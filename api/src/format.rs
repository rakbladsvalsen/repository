use actix_web::{
    delete, get, post, web,
    web::{Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    format::ModelAsQuery, sea_orm::TryIntoModel, user::Model, FormatMutation, FormatQuery,
    GetAllPaginated,
};

use entity::format::Model as FormatModel;
use log::info;

use crate::{
    common::AppState,
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResult, AsAPIResult},
    pagination::{APIPager, PaginatedResponse},
    util::verify_admin,
};

#[get("")]
async fn get_all_format(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    db: web::Data<AppState>,
) -> APIResult {
    pager.validate()?;
    let filter = filter.into_inner();
    let pager = pager.into_inner().into();
    Ok(
        PaginatedResponse::from(FormatQuery::get_all(&db.conn, &filter, &pager, None).await?)
            .into(),
    )
}

#[get("{id}")]
async fn get_format(id: Option<Path<i32>>, db: web::Data<AppState>) -> APIResult {
    let id = *id.ok_or(APIError::BadRequest)?;
    let format = FormatQuery::find_by_id(&db.conn, id)
        .await?
        .ok_or(APIError::NotFound(format!("format with ID {}", id)))?;
    HttpResponse::Ok().json(format.try_into_model()?).to_ok()
}

#[delete("{id}")]
async fn delete_format(
    id: Option<Path<i32>>,
    db: web::Data<AppState>,
    user: ReqData<Model>,
) -> APIResult {
    verify_admin(&user)?;
    let id = *id.ok_or(APIError::BadRequest)?;
    let result = FormatMutation::delete(&db.conn, id).await?;
    info!("Delete: Success: {result:?}");
    HttpResponse::NoContent().finish().to_ok()
}

#[post("")]
async fn create_format(
    inbound: Json<FormatModel>,
    db: web::Data<AppState>,
    user: ReqData<Model>,
) -> APIResult {
    verify_admin(&user)?;
    let outbound = FormatMutation::create(&db.conn, inbound.into_inner()).await?;
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
