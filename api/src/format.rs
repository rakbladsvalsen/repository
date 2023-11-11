use actix_web::{
    delete, get, post, web,
    web::{Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    format::ModelAsQuery, sea_orm::TryIntoModel, user::Model as User, FormatMutation, FormatQuery,
    GetAllPaginated, PaginationOptions,
};

use entity::format::Model as FormatModel;
use log::info;

use crate::{
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{PaginatedResponse, Validate},
    util::verify_admin,
};

#[get("")]
async fn get_all_format(
    pager: Query<PaginationOptions>,
    filter: Query<ModelAsQuery>,
    user: ReqData<User>,
) -> APIResponse {
    pager.validate()?;
    let filter = filter.into_inner();
    let pager = pager.into_inner();
    let user = user.into_inner();
    let result = FormatQuery::get_all_filtered_for_user(&filter, &pager, user, None).await?;
    Ok(PaginatedResponse::from(result).into())
}

#[get("{id}")]
async fn get_format(id: Option<Path<i32>>, user: ReqData<User>) -> APIResponse {
    let id = *id.ok_or(APIError::BadRequest)?;
    let format = FormatQuery::find_by_id(&user.into_inner(), id)
        .await?
        .ok_or(APIError::NotFound(format!("format with ID {}", id)))?;
    HttpResponse::Ok().json(format.try_into_model()?).to_ok()
}

#[delete("{id}")]
async fn delete_format(id: Option<Path<i32>>, user: ReqData<User>) -> APIResponse {
    verify_admin(&user)?;
    let id = *id.ok_or(APIError::BadRequest)?;
    let result = FormatMutation::delete(id).await?;
    info!("Delete: Success: {result:?}");
    HttpResponse::NoContent().finish().to_ok()
}

#[post("")]
async fn create_format(inbound: Json<FormatModel>, user: ReqData<User>) -> APIResponse {
    verify_admin(&user)?;
    let outbound = FormatMutation::create(inbound.into_inner()).await?;
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
