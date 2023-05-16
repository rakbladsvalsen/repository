use crate::{
    conf::DB_POOL,
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
    auth: ReqData<UserModel>,
) -> APIResult {
    pager.validate()?;
    let db = DB_POOL.get().expect("database is not initialized");
    let auth = auth.into_inner();
    let filter = filter.into_inner();
    let pager = pager.into_inner().into();
    let ul_sessions = UploadSessionQuery::get_all_for_user(db, &filter, &pager, auth).await?;
    Ok(PaginatedResponse::from(ul_sessions).into())
}

pub fn init_upload_session_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/upload_session")
        .wrap(AuthMiddleware)
        .service(get_all_upload_sessions);
    cfg.service(scope);
}
