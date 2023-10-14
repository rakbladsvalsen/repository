use actix_web::{
    delete, get, post, web,
    web::{Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    format::ModelAsQuery, sea_orm::TryIntoModel, user::Model as User, FormatMutation, FormatQuery,
};

use entity::format::Model as FormatModel;
use log::info;

use crate::{
    conf::DB_POOL,
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{APIPager, PaginatedResponse},
    util::verify_admin,
};

#[get("")]
async fn get_all_format(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    user: ReqData<User>,
) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    pager.validate()?;
    let filter = filter.into_inner();
    let pager = pager.into_inner().into();
    let user = user.into_inner();
    Ok(
        PaginatedResponse::from(FormatQuery::get_all_for_user(db, &filter, &pager, user).await?)
            .into(),
    )
}

#[get("{id}")]
async fn get_format(id: Option<Path<i32>>) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    let id = *id.ok_or(APIError::BadRequest)?;
    let format = FormatQuery::find_by_id(db, id)
        .await?
        .ok_or(APIError::NotFound(format!("format with ID {}", id)))?;
    HttpResponse::Ok().json(format.try_into_model()?).to_ok()
}

#[delete("{id}")]
async fn delete_format(id: Option<Path<i32>>, user: ReqData<User>) -> APIResponse {
    verify_admin(&user)?;
    let db = DB_POOL.get().expect("database is not initialized");
    let id = *id.ok_or(APIError::BadRequest)?;
    let result = FormatMutation::delete(db, id).await?;
    info!("Delete: Success: {result:?}");
    HttpResponse::NoContent().finish().to_ok()
}

#[post("")]
async fn create_format(inbound: Json<FormatModel>, user: ReqData<User>) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    verify_admin(&user)?;
    let outbound = FormatMutation::create(db, inbound.into_inner()).await?;
    HttpResponse::Created()
        .json(outbound.try_into_model()?)
        .to_ok()
}

pub fn init_format_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/format")
        .wrap(AuthMiddleware)
        .service(create_format)
        .service(get_all_format)
        .service(delete_format)
        .service(get_format);

    cfg.service(scope);
}
