use crate::{
    common::{timed, AppState},
    conf::BULK_INSERT_CHUNK_SIZE,
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResult, AsAPIResult},
    pagination::{APIPager, PaginatedResponse},
    record_validation::InboundRecordData,
};
use central_repository_dao::{
    record::ModelAsQuery, upload_session::OutcomeKind, user::Model as UserModel, FormatQuery,
    GetAllPaginated, RecordMutation, RecordQuery, UploadSessionMutation, UserQuery,
};

use actix_web::{
    get, post,
    web::{self, Data, Json, Query, ReqData},
    HttpResponse,
};
use entity::record::Model as RecordModel;
use entity::upload_session::Model as UploadSessionModel;
use futures::future::join_all;
use log::{debug, error, info};
use rayon::{prelude::*, slice::ParallelSlice};
use validator::Validate;

#[get("")]
async fn get_all_records(
    db: web::Data<AppState>,
    pager: Option<Query<APIPager>>,
    filter: Option<Query<ModelAsQuery>>,
    auth: ReqData<UserModel>,
) -> APIResult {
    let pager = pager.unwrap_or_else(|| actix_web::web::Query(APIPager::default()));
    pager.validate()?;
    let pager = pager.into_inner();
    let auth = auth.into_inner();
    let filter = filter
        .unwrap_or_else(|| web::Query(ModelAsQuery::default()))
        .into_inner();
    let records = match auth.is_superuser {
        true => RecordQuery::get_all(&db.conn, &filter, pager.page, pager.per_page, None).await,
        _ => {
            info!("filtering available records for user id: {:?}", auth.id);
            RecordQuery::get_all_for_user(&db.conn, &filter, pager.page, pager.per_page, auth).await
        }
    };
    Ok(PaginatedResponse::from(records?).into())
}

#[post("")]
async fn create_record(
    inbound: Json<InboundRecordData>,
    db: Data<AppState>,
    auth: ReqData<UserModel>,
) -> APIResult {
    let auth = auth.into_inner();
    let request_item_length = inbound.data.len() as i32;
    let inbound = inbound.into_inner();

    let format = match auth.is_superuser {
        // bypass format check for superusers
        true => FormatQuery::find_by_id(&db.conn, inbound.format_id)
            .await?
            .ok_or_else(|| APIError::NotFound(format!("format with ID {}", inbound.format_id)))?,
        // for normal users, check if they can write to this format
        false => UserQuery::find_writable_format(&db.conn, &auth, inbound.format_id)
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
    let payload_validation = timed!(
        "validation of json data",
        actix_web::web::block(move || {
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
    let upload_session = UploadSessionMutation::create(&db.conn, upload_session).await?;

    let saved_entries = match payload_validation {
        Ok(inbound) => {
            let request_entries = inbound.data.len() as u64;
            let entries = inbound
                .data
                .into_par_iter()
                .map(|entry| RecordModel::new(upload_session.id, entry))
                .collect::<Vec<_>>();
            let insert_tasks = entries
                .par_chunks(*BULK_INSERT_CHUNK_SIZE)
                .into_par_iter()
                .map(|chunk| RecordMutation::create_many(&db.conn, chunk.to_owned()))
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
                &db.conn,
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
        .service(get_all_records)
        .service(create_record);

    cfg.service(scope);
}
