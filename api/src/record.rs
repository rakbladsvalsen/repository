use crate::{
    common::{timed, DebugMode},
    conf::APIConfig,
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{PaginatedResponse, Validate},
    record_validation::InboundRecordData,
};
use central_repository_config::inner::Config;
use central_repository_dao::{
    record::ModelAsQuery, upload_session::OutcomeKind, user::Model as UserModel, FormatQuery,
    PaginationOptions, ParallelStreamConfig, RecordMutation, RecordQuery, SearchQuery,
    UploadSessionMutation, UserQuery,
};

use actix_web::{
    post,
    web::{self, Json, Query, ReqData},
    HttpResponse,
};
use entity::record::Model as RecordModel;
use entity::upload_session::Model as UploadSessionModel;
use futures::{future::join_all, StreamExt};
use log::{debug, error, info};
use rayon::{prelude::*, slice::ParallelSlice};

#[post("/filter")]
async fn get_all_filtered_records(
    pager: Query<PaginationOptions>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
    query: Json<SearchQuery>,
    debug: actix_web::web::Query<DebugMode>,
) -> APIResponse {
    query.validate()?;
    // get this query's inner contents
    let query = query.into_inner();
    info!("query: {:#?}", query);
    let filter = filter.into_inner();
    pager.validate()?;
    let pager = pager.into_inner();
    if **debug {
        // if "?debug=true" is passed, return the full dict query
        info!("accessed debugging interface");
        return HttpResponse::Ok().json(query).to_ok();
    }
    let prepared_search = query.get_readable_formats_for_user(&auth).await?;
    // create extra filtering condition to search inside ALL JSONB hashmaps
    let records = RecordQuery::filter_readable_records(&filter, &pager, prepared_search).await?;
    Ok(PaginatedResponse::from(records).into())
}

#[post("/filter-stream")]
async fn get_all_filtered_records_stream(
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
    query: Json<SearchQuery>,
    debug: actix_web::web::Query<DebugMode>,
) -> APIResponse {
    query.validate()?;
    // get this query's inner contents
    let query = query.into_inner();
    info!("query: {:#?}", query);
    let filter = filter.into_inner();
    if **debug {
        info!("accessed debugging interface");
        return HttpResponse::Ok().json(query).to_ok();
    }

    let config = ParallelStreamConfig::default();

    let mut limit_grant = None;
    if !auth.is_superuser {
        limit_grant = Some(APIConfig::get_limit_service().new_grant_for_key(&auth.username)?);
    }

    let stream = RecordQuery::filter_readable_records_stream(
        auth.into_inner(),
        &filter,
        query,
        config,
        limit_grant,
    )
    .await?
    .map(|it| Ok::<_, APIError>(web::Bytes::from(it)));

    HttpResponse::Ok()
        .append_header(("Content-Type", "text/csv"))
        .streaming(stream)
        .to_ok()
}

#[post("")]
async fn create_record(inbound: Json<InboundRecordData>, auth: ReqData<UserModel>) -> APIResponse {
    let auth = auth.into_inner();
    let request_item_length = inbound.data.len() as i32;
    let inbound = inbound.into_inner();
    let format = match auth.is_superuser {
        // bypass format check for superusers
        true => FormatQuery::find_by_id(&auth, inbound.format_id)
            .await?
            .ok_or_else(|| APIError::NotFound(format!("format with ID {}", inbound.format_id)))?,
        // for normal users, check if they can write to this format
        false => UserQuery::find_writable_format(&auth, inbound.format_id)
            .await?
            .ok_or_else(|| {
                info!(
                    "User {} doesn't have write permissions on format {}",
                    auth.id, inbound.format_id
                );
                APIError::InsufficientPermissions
            })?,
    };
    let format_id = format.id;
    let current_span = tracing::Span::current();
    let payload_validation = timed!(
        "validation of json data",
        actix_web::web::block(move || {
            // Enter the current logging span (we'll be running in another thread)
            let _guard = current_span.enter();
            // Validate the entire payload without blocking the main thread. If validation
            // succeeds, we just return the data again (web::block takes ownership of the
            // moved data).
            inbound.validate_blocking(&format).map(|_| inbound)
        })
        .await?
    );

    let outcome_detail = match payload_validation.as_ref() {
        Ok(_) => (
            OutcomeKind::Success,
            format!(
                "User ID {} uploaded {} entries",
                auth.id, request_item_length
            ),
        ),
        Err(err) => (OutcomeKind::Error, err.to_string()),
    };

    // upload session data
    let upload_session = UploadSessionModel {
        format_id,
        user_id: auth.id,
        record_count: request_item_length,
        outcome: outcome_detail.0,
        detail: outcome_detail.1,
        ..Default::default()
    };
    let upload_session = UploadSessionMutation::create(upload_session).await?;

    let saved_entries = match payload_validation {
        Ok(inbound) => {
            let request_entries = inbound.data.len() as u64;
            let entries = inbound
                .data
                .into_par_iter()
                .map(|entry| RecordModel::new(upload_session.id, format_id, entry))
                .collect::<Vec<_>>();
            let insert_tasks = entries
                .par_chunks(Config::get().bulk_insert_chunk_size as usize)
                .into_par_iter()
                .map(|chunk| RecordMutation::create_many(chunk.to_owned()))
                .collect::<Vec<_>>();
            info!(
                "Preparing {request_entries} entries/{} chunks = {} insert jobs.",
                Config::get().bulk_insert_chunk_size,
                insert_tasks.len()
            );
            join_all(insert_tasks)
                .await
                .into_iter()
                .collect::<Result<Vec<u64>, _>>()
        }
        Err(err) => return Err(err),
    };

    // verify whether we were able to save ALL the records successfully.
    match saved_entries {
        Ok(_) => {
            info!(
                "Successfully saved {request_item_length} entries for format {}.",
                format_id
            );
            HttpResponse::Ok().json(upload_session).to_ok()
        }
        // there was an error, roll back the SUCCESS status to a FAILED one
        // (this should never happen).
        Err(err) => {
            error!("Rolling back success status to error (caused by: {err:?})");
            UploadSessionMutation::update_as_failed(
                upload_session.id,
                format!("{:?}, {:?}", err, err.to_string()),
            )
            .await
            .map(|_| Err(APIError::ServerError))?
        }
    }
}

pub fn init_record_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/record")
        .wrap(AuthMiddleware)
        // .service(get_all_records)
        .service(create_record)
        .service(get_all_filtered_records)
        .service(get_all_filtered_records_stream);

    cfg.service(scope);
}
