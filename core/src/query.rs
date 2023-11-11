use std::sync::Arc;

use crate::{
    conf::DBConfig, pagination_impl::GetAllTrait, CoreError, GetAllPaginated, LimitGrant,
    PaginationOptions, PreparedSearchQuery, SearchQuery,
};
use ::entity::{
    api_key,
    error::DatabaseQueryError,
    format,
    format::Entity as Format,
    format_entitlement::{
        self, AccessLevel, SearchModel as FormatEntitlementSearch, ARRAY_CONTAINS_OP,
    },
    record, upload_session, user,
    user::Entity as User,
};
use async_stream::stream;
use central_repository_config::inner::Config;
use futures::{Stream, StreamExt};
use log::{debug, info};
use sea_orm::*;
use sea_query::Expr;
use tracing::Span;
use uuid::Uuid;

// Fixed headers for CSV exports
const FIXED_HEADERS: &str = r#""ID","FormatId","UploadSessionId""#;

// Query objects
pub struct UploadSessionQuery;
pub struct FormatEntitlementQuery;
pub struct FormatQuery;
pub struct UserQuery;
pub struct RecordQuery;

pub struct ApiKeyQuery;

pub struct ParallelStreamConfig {
    num_streams: usize,
    num_queue_items: usize,
    num_transform_threads: usize,
}

impl ParallelStreamConfig {
    pub fn new(num_streams: usize, num_queue_items: usize, num_transform_threads: usize) -> Self {
        Self {
            num_streams,
            num_queue_items,
            num_transform_threads,
        }
    }
}

impl Default for ParallelStreamConfig {
    fn default() -> Self {
        let conf = Config::get();
        Self::new(
            conf.db_csv_stream_workers as usize,
            conf.db_csv_worker_queue_depth as usize,
            conf.db_csv_transform_workers as usize,
        )
    }
}

impl GetAllTrait<'_> for UserQuery {
    type Entity = user::Entity;
    type FilterQueryModel = user::ModelAsQuery;
    type ResultModel = user::Model;
}

impl GetAllTrait<'_> for FormatQuery {
    type Entity = format::Entity;
    type FilterQueryModel = format::ModelAsQuery;
    type ResultModel = format::Model;

    fn filter_out_select(
        user: &user::Model,
        select: Select<Self::Entity>,
    ) -> sea_orm::Select<Self::Entity> {
        if !user.is_superuser {
            info!("filtering available formats for user {:?}", user.id);
            let filter = format_entitlement::Entity::find()
                .select_only()
                .column(format_entitlement::Column::FormatId)
                .filter(format_entitlement::Column::UserId.eq(user.id));
            let subquery = filter.as_query();

            return select.filter(format::Column::Id.in_subquery(subquery.to_owned()));
        }
        select
    }
}

impl GetAllTrait<'_> for FormatEntitlementQuery {
    type FilterQueryModel = format_entitlement::ModelAsQuery;
    type ResultModel = format_entitlement::Model;
    type Entity = format_entitlement::Entity;

    fn filter_out_select(
        user: &user::Model,
        select: Select<Self::Entity>,
    ) -> sea_orm::Select<Self::Entity> {
        if !user.is_superuser {
            return select.filter(format_entitlement::Column::UserId.eq(user.id));
        }
        select
    }
}

impl GetAllTrait<'_> for UploadSessionQuery {
    type FilterQueryModel = upload_session::ModelAsQuery;
    type ResultModel = upload_session::Model;
    type Entity = upload_session::Entity;

    fn filter_out_select(
        user: &user::Model,
        select: Select<Self::Entity>,
    ) -> sea_orm::Select<Self::Entity> {
        if !user.is_superuser {
            let formats_for_user = format_entitlement::Entity::find()
                .select_only()
                .column(format_entitlement::Column::FormatId)
                .filter(format_entitlement::Column::UserId.eq(user.id));
            let format_for_user_subquery = formats_for_user.as_query();
            return select.filter(
                upload_session::Column::FormatId.in_subquery(format_for_user_subquery.to_owned()),
            );
        }
        select
    }
}

impl GetAllTrait<'_> for RecordQuery {
    type FilterQueryModel = record::ModelAsQuery;
    type ResultModel = record::Model;
    type Entity = record::Entity;
}

impl GetAllTrait<'_> for ApiKeyQuery {
    type FilterQueryModel = api_key::ModelAsQuery;
    type ResultModel = api_key::Model;
    type Entity = api_key::Entity;

    fn filter_out_select(
        user: &user::Model,
        mut select: Select<Self::Entity>,
    ) -> sea_orm::Select<Self::Entity> {
        if !user.is_superuser {
            info!("filtering available keys for user {:?}", user.id);
            select = select.filter(api_key::Column::UserId.eq(user.id));
        }
        select
    }
}

// Custom impl's for weird usecases.

impl RecordQuery {
    // Get all available records.
    pub async fn filter_readable_records(
        filters: &record::ModelAsQuery,
        pagination_options: &PaginationOptions,
        prepared_search: PreparedSearchQuery,
    ) -> Result<(Vec<record::Model>, u64, u64), DatabaseQueryError> {
        let select = prepared_search.apply_condition(record::Entity::find())?;
        RecordQuery::get_all(filters, pagination_options, Some(select))
            .await
            .map_err(DatabaseQueryError::from)
    }

    pub async fn filter_readable_records_stream(
        auth: user::Model,
        filters: &record::ModelAsQuery,
        query: SearchQuery,
        parallel_stream_config: ParallelStreamConfig,
        limit_grant: Option<LimitGrant>,
    ) -> Result<impl Stream<Item = Vec<u8>>, CoreError> {
        let db = DBConfig::get_connection();
        let prepared_search = query
            .get_readable_formats_for_user(&auth)
            .await
            .map_err(DatabaseQueryError::from)?;
        let schema_columns = Arc::new(prepared_search.schema_columns());

        let mut headers = schema_columns
            .iter()
            .map(|col| format!("{:?}", col))
            .collect::<Vec<_>>()
            .join(",");
        headers = format!("{FIXED_HEADERS},{headers}\n");

        // apply conditions and filters.
        let mut select = record::Entity::find().order_by_asc(record::Column::Id);
        select = prepared_search.apply_condition(select)?;
        select = RecordQuery::apply_filters(filters, Some(select));

        let mut limit = None;

        // If there's only 1 stream, it doesn't make sense to use multiple database streams,
        // and therefore it doesn't make any sense to issue a COUNT query.
        if parallel_stream_config.num_streams > 1 {
            debug!(
                "streaming: using {} streams, now waiting for COUNT",
                parallel_stream_config.num_streams
            );
            let num_items = RecordQuery::num_items(&mut (select.clone()))
                .await
                .map_err(DatabaseQueryError::from)?;

            limit =
                Some((num_items as f64 / parallel_stream_config.num_streams as f64).ceil() as u64); // page size
            debug!(
                "streaming: COUNT returned {} items / {} streams = {:?} items per page",
                num_items, parallel_stream_config.num_streams, limit
            );
        } else {
            debug!("streaming: not issuing COUNT as there's only 1 stream");
        }

        let (tx_db_stream, rx_db_stream) = flume::bounded(parallel_stream_config.num_queue_items);
        let (tx_result, rx_result) = flume::bounded(parallel_stream_config.num_queue_items);

        // Spawn receiving threads
        for stream_thread in 0..parallel_stream_config.num_streams {
            let offset = stream_thread as u64 * limit.unwrap_or(0);
            let thread_select = select.clone().limit(limit).offset(offset);
            debug!("stream worker {stream_thread}: offset: {offset} limit: {limit:?}");
            let thread_tx_db_stream = tx_db_stream.clone();
            tokio::spawn(async move {
                let mut received = 0;
                let mut stream = thread_select.stream(db).await?;
                while let Some(Ok(item)) = stream.next().await {
                    received += 1;
                    if thread_tx_db_stream.send_async(item).await.is_err() {
                        break;
                    }
                }
                debug!("stream_thread: {stream_thread}: received {received} items");
                Ok::<_, DatabaseQueryError>(())
            });
        }
        drop(tx_db_stream);

        for transform_thread in 0..parallel_stream_config.num_transform_threads {
            let rx_db_stream_thread = rx_db_stream.clone();
            let tx_result_thread = tx_result.clone();
            let schema_columns_thread = schema_columns.clone();
            tokio::spawn(async move {
                let mut processed = 0;
                while let Ok(item) = rx_db_stream_thread.recv_async().await {
                    processed += 1;
                    let mut row = schema_columns_thread
                        .iter()
                        .map(|column| {
                            item.data
                                .get(column)
                                .map_or("".into(), |value| format!("{}", value))
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    // Build CSV row.
                    row = format!(
                        "{},{},{},{row}\n",
                        item.id, item.format_id, item.upload_session_id
                    );
                    if tx_result_thread.send_async(row.into_bytes()).await.is_err() {
                        break;
                    }
                }
                debug!("transform_thread {transform_thread}: processed {processed} items");
                Ok::<_, DatabaseQueryError>(())
            });
        }
        drop(tx_result);
        drop(rx_db_stream);

        let current_span = Span::current();

        Ok(stream!({
            let _guard = current_span.enter();
            // Capture user grant for this streaming operation
            let _limit_grant = limit_grant;

            yield headers.into_bytes();

            while let Ok(item) = rx_result.recv_async().await {
                yield item;
            }

            info!("finished streaming");
        }))
    }
}

impl FormatQuery {
    pub async fn find_by_id(user: &user::Model, id: i32) -> Result<Option<format::Model>, DbErr> {
        let db = DBConfig::get_connection();
        Self::filter_out_select(user, Format::find_by_id(id))
            .one(db)
            .await
    }
}

impl UserQuery {
    pub async fn find_by_id(id: uuid::Uuid) -> Result<Option<user::Model>, DbErr> {
        let db = DBConfig::get_connection();
        User::find().filter(user::Column::Id.eq(id)).one(db).await
    }

    pub async fn find_nonsuperuser_by_id(id: uuid::Uuid) -> Result<Option<user::Model>, DbErr> {
        let db = DBConfig::get_connection();
        User::find()
            .filter(user::Column::Id.eq(id))
            .filter(user::Column::IsSuperuser.eq(false))
            .one(db)
            .await
    }

    pub async fn find_by_username(username: &String) -> Result<Option<user::Model>, DbErr> {
        let db = DBConfig::get_connection();
        User::find()
            .filter(user::Column::Username.eq(username))
            .one(db)
            .await
    }

    /// Verify whether the passed user has write access to `fmt` (a format).
    #[inline(always)]
    pub async fn find_writable_format(
        user: &user::Model,
        format_id: i32,
    ) -> Result<Option<format::Model>, DbErr> {
        let db = DBConfig::get_connection();
        let col = Expr::col(format_entitlement::Column::Access);
        user.find_related(format::Entity)
            .filter(format::Column::Id.eq(format_id))
            .filter(
                // Filter only formats this user can write to
                col.binary(
                    ARRAY_CONTAINS_OP,
                    AccessLevel::Write.get_serialized().as_str(),
                ),
            )
            .one(db)
            .await
    }
}

impl FormatEntitlementQuery {
    pub async fn find_by_id(
        id: &FormatEntitlementSearch,
    ) -> Result<Option<format_entitlement::Model>, DbErr> {
        let db = DBConfig::get_connection();
        format_entitlement::Entity::find_by_id((id.user_id, id.format_id))
            .one(db)
            .await
    }
}

impl ApiKeyQuery {
    /// Get the user associated with the given `user_id` and all its related
    /// keys.
    pub async fn get_user_and_keys(
        user_id: Uuid,
    ) -> Result<Option<(user::Model, Vec<api_key::Model>)>, DbErr> {
        let db = DBConfig::get_connection();
        let mut first = user::Entity::find_by_id(user_id)
            .find_with_related(api_key::Entity)
            .all(db)
            .await?;
        if first.is_empty() {
            return Ok(None);
        }
        Ok(Some(first.remove(0)))
    }

    /// Get the user associated with the given `user_id` and the related key.
    pub async fn get_user_and_single_key(
        user_id: Uuid,
        key_id: Uuid,
    ) -> Result<Option<(user::Model, api_key::Model)>, DbErr> {
        let db = DBConfig::get_connection();
        let mut first = user::Entity::find_by_id(user_id)
            .find_with_related(api_key::Entity)
            .filter(api_key::Column::Id.eq(key_id))
            .all(db)
            .await?;
        if first.is_empty() {
            return Ok(None);
        }
        let (user, mut key) = first.remove(0);
        debug!(
            "api key query returned {} key(s) for user {}",
            key.len(),
            user.id
        );
        // if `key` doesn't have anything, then the passed key does not exist.
        if key.is_empty() {
            return Ok(None);
        }
        Ok(Some((user, key.remove(0))))
    }
}
