use crate::{
    common::AppState,
    core_middleware::auth::AuthMiddleware,
    error::APIResult,
    pagination::{APIPager, PaginatedResponse},
};
use actix_web::{
    get,
    web::{self, Query, ReqData},
};
use central_repository_dao::{
    upload_session::ModelAsQuery, user::Model as UserModel, UploadSessionQuery,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
}

#[get("")]
async fn get_all_upload_sessions(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    db: web::Data<AppState>,
    auth: ReqData<UserModel>,
) -> APIResult {
    pager.validate()?;
    let auth = auth.into_inner();
    let filter = filter.into_inner();
    let ul_sessions =
        UploadSessionQuery::get_all_for_user(&db.conn, &filter, pager.page, pager.per_page, auth)
            .await?;
    Ok(PaginatedResponse::from(ul_sessions).into())
}

pub fn init_upload_session_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/upload_session")
        .wrap(AuthMiddleware)
        .service(get_all_upload_sessions);
    cfg.service(scope);
}
