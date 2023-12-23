use crate::{
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{PaginatedResponse, Validate},
    util::verify_admin,
};
use actix_web::{
    delete, get, post,
    web::{self, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    upload_session::ModelAsQuery, user::Model as UserModel, GetAllPaginated, PaginationOptions,
    UploadSessionMutation, UploadSessionQuery,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
}

#[get("")]
async fn get_all_upload_sessions(
    pager: Query<PaginationOptions>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
) -> APIResponse {
    pager.validate()?;
    let auth = auth.into_inner();
    let filter = filter.into_inner();
    let pager = pager.into_inner();
    let items = UploadSessionQuery::get_all_filtered_for_user(&filter, &pager, auth, None).await?;
    Ok(PaginatedResponse::from(items).into())
}

#[delete("{id}")]
async fn delete(auth: ReqData<UserModel>, id: Option<Path<i32>>) -> APIResponse {
    let id = *id.ok_or(APIError::BadRequest)?;
    let auth = auth.into_inner();
    UploadSessionMutation::delete(auth, id).await?;
    HttpResponse::NoContent().finish().to_ok()
}

#[post("/prune")]
async fn prune(auth: ReqData<UserModel>) -> APIResponse {
    verify_admin(&auth)?;
    let result = UploadSessionMutation::prune_old_items().await?;
    HttpResponse::Ok().json(result).to_ok()
}

pub fn init_upload_session_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/upload_session")
        .wrap(AuthMiddleware)
        .service(get_all_upload_sessions)
        .service(prune)
        .service(delete);
    cfg.service(scope);
}
