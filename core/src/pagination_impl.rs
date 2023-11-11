use crate::{conf::DBConfig, traits::*};
use ::entity::user;
use async_trait::async_trait;
use central_repository_config::inner::Config;
use futures::{try_join, Stream};
use log::{debug, info};
use sea_orm::*;
use sea_query::{Alias, Expr, SelectStatement};
use serde::Deserialize;
use std::{fmt::Debug, pin::Pin};

type ResultModel<T> = Result<T, DbErr>;

/// This trait provides sorted + filtered + paginated searches
/// for any type implementing the 3 associated types.
pub trait GetAllTrait<'db> {
    type ResultModel: FromQueryResult + Sized + Send + Sync + 'db;
    type FilterQueryModel: AsQueryParamFilterable + AsQueryParamSortable + Debug + Send + Sync;
    type Entity: EntityTrait<Model = Self::ResultModel>;

    /// Filter objects for on a per-user basis. This trait allows the caller
    /// to modify the database query in order to filter out potentially sensitive entries.
    #[inline(always)]
    fn filter_out_select(
        _user: &user::Model,
        select: Select<Self::Entity>,
    ) -> sea_orm::Select<Self::Entity> {
        select
    }
}

fn default_page() -> u64 {
    0
}

fn default_page_size() -> u64 {
    Config::get().default_pagination_size
}

fn default_full_count() -> bool {
    Config::get().return_query_count
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaginationOptions {
    /// The page to fetch.
    #[serde(default = "default_page")]
    pub page: u64,
    /// The number of items per page.
    #[serde(default = "default_page_size")]
    pub per_page: u64,
    /// Whether to fetch items and page count.
    #[serde(default = "default_full_count")]
    pub count: bool,
}

impl PaginationOptions {
    /// Whether the passed pagination options are valid or not.
    #[inline(always)]
    pub fn is_valid(&self) -> bool {
        self.per_page > 0 && self.per_page <= Config::get().max_pagination_size
    }
}

#[async_trait]
pub trait GetAllPaginated<'db>: GetAllTrait<'db> {
    /// Get total number of items in this query.
    #[inline(always)]
    async fn num_items(select: &mut sea_orm::Select<Self::Entity>) -> Result<u64, DbErr> {
        let db = DBConfig::get_connection();
        let stmt = SelectStatement::new()
            .expr(Expr::cust("COUNT (*) AS num_items"))
            .from_subquery(
                sea_orm::QueryTrait::query(select).to_owned(),
                Alias::new("sub_query"),
            )
            .to_owned();

        let stmt = StatementBuilder::build(&stmt, &sea_orm::DatabaseBackend::Postgres);

        let result = db.query_all(stmt).await?;
        let result = match result.get(0) {
            Some(i) => i,
            _ => return Ok(0),
        };
        Ok(result.try_get::<i64>("", "num_items")? as u64)
    }

    /// Get number of items and pages.
    /// There's already a built-in SeaORM method with the same name
    /// (num_items_and _pages) but it has a weird bug for some reason
    /// that causes the COUNT call to be terribly slow.
    ///
    /// See more: https://github.com/SeaQL/sea-orm/issues/1888
    ///
    /// Returns a tuple, containing:
    /// - (Total number of pages, total number of items)
    #[inline(always)]
    async fn num_items_and_pages(
        select: &mut sea_orm::Select<Self::Entity>,
        page_size: u64,
    ) -> Result<(u64, u64), DbErr> {
        let num_items = Self::num_items(select).await?;
        let num_pages = (num_items as f64 / page_size as f64).ceil() as u64;
        Ok((num_pages, num_items))
    }

    /// Apply default sorting column and, additionally, filter
    /// using any fields the user passed.
    fn apply_filters(
        filters: &Self::FilterQueryModel,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Select<Self::Entity> {
        debug!("filter params: {:#?}", filters);
        let mut select_stmt = select_stmt.unwrap_or_else(Self::Entity::find);
        select_stmt = filters.filter(select_stmt);
        select_stmt = filters.sort(select_stmt);
        select_stmt
    }

    /// Get all available items as a stream.
    ///
    /// This will apply the query and return an unpaged stream of items.
    /// Parameters:
    ///
    /// `db`: The database connection.
    /// filtters: Filters to apply to the query.
    /// select_stmt: Optional statement to use for the query.
    async fn get_all_as_stream(
        filters: &Self::FilterQueryModel,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Result<Pin<Box<dyn Stream<Item = ResultModel<Self::ResultModel>> + Send + 'db>>, DbErr>
    {
        let db = DBConfig::get_connection();
        let select = Self::apply_filters(filters, select_stmt);
        Ok(select.stream(db).await.map(Box::pin)?)
    }

    /// Get all available items using pagination.
    async fn get_all(
        filters: &Self::FilterQueryModel,
        pagination_options: &PaginationOptions,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Result<(Vec<Self::ResultModel>, u64, u64), DbErr> {
        let db = DBConfig::get_connection();
        debug!("pagination options: {:#?}", pagination_options);
        let mut select = Self::apply_filters(filters, select_stmt);
        let select_ordered = select.clone();

        // Create paginators.
        // Note that the ordered paginator only returns items sorted by whatever column was passed in order_by,
        // for the actual count we don't need to ORDER BY the internal query.
        let paginator_ordered = select_ordered.paginate(db, pagination_options.per_page);
        let pagination_fut = paginator_ordered.fetch_page(pagination_options.page);

        if pagination_options.count {
            info!("executing potentially slow query");
            // let paginator = select.paginate(db, pagination_options.page_size);
            let items_and_pages_fut =
                Self::num_items_and_pages(&mut select, pagination_options.per_page);
            // if items and pages is enabled, run two queries concurrently:
            // - a normal SELECT query
            // - a COUNT(*) query
            let (items, (num_pages, num_items)) = try_join!(pagination_fut, items_and_pages_fut)?;
            return Ok((items, num_pages, num_items));
        }
        // if items and pages is disabled, run a single query
        Ok((pagination_fut.await?, 0, 0))
    }

    /// Get all entries filtered for this user.
    async fn get_all_filtered_for_user(
        filters: &Self::FilterQueryModel,
        pagination_options: &PaginationOptions,
        user: user::Model,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Result<(Vec<Self::ResultModel>, u64, u64), DbErr> {
        let mut select_stmt = select_stmt.unwrap_or_else(Self::Entity::find);
        select_stmt = Self::filter_out_select(&user, select_stmt);
        Self::get_all(filters, pagination_options, Some(select_stmt)).await
    }
}

#[async_trait]
impl<'db, T> GetAllPaginated<'db> for T where T: GetAllTrait<'db> {}
