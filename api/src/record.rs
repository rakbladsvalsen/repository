use crate::{
    common::{timed, DebugMode},
    conf::{BULK_INSERT_CHUNK_SIZE, DB_CSV_STREAM_WORKERS, DB_CSV_WORKER_QUEUE_DEPTH, DB_POOL},
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResult, AsAPIResult},
    pagination::{APIPager, PaginatedResponse},
    record_validation::InboundRecordData,
};
use async_stream::stream;
use central_repository_dao::{
    record::ModelAsQuery, upload_session::OutcomeKind, user::Model as UserModel, FormatQuery,
    RecordMutation, RecordQuery, SearchQuery, UploadSessionMutation, UserQuery,
};
use std::sync::Arc;

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

const FIXED_HEADERS: &str = r#""ID","FormatId","UploadSessionId""#;

#[post("/filter")]
async fn get_all_filtered_records(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
    query: Json<SearchQuery>,
    debug: actix_web::web::Query<DebugMode>,
) -> APIResult {
    query.validate()?;
    let db = DB_POOL.get().expect("database is not initialized");
    // get this query's inner contents
    let query = query.into_inner();
    info!("query: {:#?}", query);
    let filter = filter.into_inner();
    pager.validate()?;
    let pager = pager.into_inner().into();
    if **debug {
        // if "?debug=true" is passed, return the full dict query
        info!("accessed debugging interface");
        return HttpResponse::Ok().json(query).to_ok();
    }
    let prepared_search = query.get_readable_formats_for_user(&auth, db).await?;
    // create extra filtering condition to search inside ALL JSONB hashmaps
    let records =
        RecordQuery::filter_readable_records(db, &filter, &pager, prepared_search).await?;
    Ok(PaginatedResponse::from(records).into())
}

#[post("/filter-stream")]
async fn get_all_filtered_records_stream(
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
    query: Json<SearchQuery>,
    debug: actix_web::web::Query<DebugMode>,
) -> APIResult {
    query.validate()?;
    // get this query's inner contents
    let query = query.into_inner();
    let filter = filter.into_inner();
    let db = DB_POOL.get().expect("database is not initialized");
    if **debug {
        info!("accessed debugging interface");
        return HttpResponse::Ok().json(query).to_ok();
    }
    let prepared_search = query.get_readable_formats_for_user(&auth, db).await?;
    let schema_columns = Arc::new(prepared_search.schema_columns());

    let mut iterator =
        RecordQuery::filter_readable_records_stream(db, &filter, prepared_search).await?;

    let (tx_model, rx_model) = flume::bounded(*DB_CSV_WORKER_QUEUE_DEPTH);
    let (tx_result, rx_result) = flume::bounded(*DB_CSV_WORKER_QUEUE_DEPTH);

    // Process database results in parallel.
    //
    // A database stream can easily overwhelm a single core when we're
    // converting the results to a CSV-compatible format. This stream
    // will spawn several worker threads that receive that data in parallel
    // and then yield that data.
    //
    // See the diagram below for more details.
    //
    //                 -> Worker 1/CSV out->
    //  DB stream ---> -> Worker 2/CSV out -> ---> Unordered CSV stream
    //                 -> Worker 3/CSV out ->

    // DB_CSV_STREAM_WORKERS
    let stream = stream! {
        let headers = schema_columns
            .iter()
            .map(|col| format!("{:?}", col))
            .collect::<Vec<_>>()
            .join(",");


        // Yield CSV headers
        yield Ok::<_, APIError>(web::Bytes::from(format!("{FIXED_HEADERS},{headers}\n")));

        // Relay database data in another thread
        tokio::spawn(async move{
            while let Some(Ok(item)) = iterator.next().await{
                tx_model.send_async(item).await?;
            }
            Ok::<_, APIError>(())
        });

        // Receive database data in multiple threads and convert it to csv.
        for worker in 0..(*DB_CSV_STREAM_WORKERS) {
            let rx_model_thread = rx_model.clone();
            let tx_result_thread = tx_result.clone();
            let schema_columns_thread = schema_columns.clone();

            tokio::spawn(async move {
                let mut processed = 0;
                while let Ok(item) = rx_model_thread.recv_async().await {
                    processed += 1;
                    let row = schema_columns_thread
                        .iter()
                        .map(|column| {
                            item.data
                                .get(column)
                                .map_or("".into(), |value| format!("{}", value))
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    tx_result_thread
                        .send_async(format!(
                            "{},{},{},{row}\n",
                            item.id, item.format_id, item.upload_session_id
                        ))
                        .await?;
                }
                info!("worker {worker}: processed {processed} items");
                Ok::<_, APIError>(())
            });
        }
        // we don't need the rx_model and tx_result halves from outside the worker threads
        drop(rx_model);
        drop(tx_result);
        while let Ok(item) = rx_result.recv_async().await{
            yield Ok::<_, APIError>(web::Bytes::from(item));
        }
    };
    HttpResponse::Ok()
        .append_header(("Content-Type", "text/csv"))
        .streaming(stream)
        .to_ok()
}

#[post("")]
async fn create_record(inbound: Json<InboundRecordData>, auth: ReqData<UserModel>) -> APIResult {
    let db = DB_POOL.get().expect("database is not initialized");
    let auth = auth.into_inner();
    let request_item_length = inbound.data.len() as i32;
    let inbound = inbound.into_inner();
    let format = match auth.is_superuser {
        // bypass format check for superusers
        true => FormatQuery::find_by_id(db, inbound.format_id)
            .await?
            .ok_or_else(|| APIError::NotFound(format!("format with ID {}", inbound.format_id)))?,
        // for normal users, check if they can write to this format
        false => UserQuery::find_writable_format(db, &auth, inbound.format_id)
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
    let upload_session = UploadSessionMutation::create(db, upload_session).await?;

    let saved_entries = match payload_validation {
        Ok(inbound) => {
            let request_entries = inbound.data.len() as u64;
            let entries = inbound
                .data
                .into_par_iter()
                .map(|entry| RecordModel::new(upload_session.id, format_id, entry))
                .collect::<Vec<_>>();
            let insert_tasks = entries
                .par_chunks(*BULK_INSERT_CHUNK_SIZE)
                .into_par_iter()
                .map(|chunk| RecordMutation::create_many(db, chunk.to_owned()))
                .collect::<Vec<_>>();
            debug!(
                "Preparing {request_entries} entries/{} chunks = {} insert jobs.",
                *BULK_INSERT_CHUNK_SIZE,
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
                db,
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
