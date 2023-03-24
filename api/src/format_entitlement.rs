use actix_example_core::{
    format_entitlement::{ModelAsQuery, SearchModel as FormatEntitlementSearch},
    sea_orm::ModelTrait,
    user::Model,
    FormatEntitlementMutation, FormatEntitlementQuery, FormatQuery, UserQuery,
};
use actix_web::{
    delete, get, post, web,
    web::{Json, Query, ReqData},
    HttpResponse,
};

use entity::format_entitlement::Model as FormatEntitlementModel;
use log::info;
use validator::Validate;

use crate::{
    common::AppState,
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResult, AsAPIResult},
    pagination::{APIPager, PaginatedResponse},
    util::verify_admin,
};

#[post("")]
async fn create_entitlement(
    inbound: Json<FormatEntitlementModel>,
    db: web::Data<AppState>,
    auth: ReqData<Model>,
) -> APIResult {
    verify_admin(&auth)?;
    // make sure we're assigning a format to a non-superuser
    UserQuery::find_nonsuperuser_by_id(&db.conn, inbound.user_id)
        .await?
        .ok_or_else(|| {
            info!("Couldn't find user id {}", inbound.user_id);
            APIError::NotFound(format!("non-superuser with ID {}", inbound.user_id))
        })?;
    // make sure this format exists before creating the entitlement
    FormatQuery::find_by_id(&db.conn, inbound.format_id)
        .await?
        .ok_or_else(|| {
            info!("Couldn't find format id {}", inbound.format_id);
            APIError::NotFound(format!("format with ID {}", inbound.format_id))
        })?;
    HttpResponse::Ok()
        .json(FormatEntitlementMutation::create(&db.conn, inbound.into_inner()).await?)
        .to_ok()
}

#[get("")]
async fn get_all_format(
    pager: Option<Query<APIPager>>,
    filter: Option<Query<ModelAsQuery>>,
    db: web::Data<AppState>,
) -> APIResult {
    let pager = *pager.unwrap_or(web::Query(APIPager::default()));
    pager.validate()?;
    let filter = filter
        .unwrap_or_else(|| web::Query(ModelAsQuery::default()))
        .into_inner();
    info!("search params: {:?}", filter);
    Ok(PaginatedResponse::from(
        FormatEntitlementQuery::get_all(&db.conn, filter, pager.page, pager.per_page).await?,
    )
    .into())
}

#[delete("")]
async fn delete_entitlement(
    inbound: Json<FormatEntitlementSearch>,
    db: web::Data<AppState>,
    auth: ReqData<Model>,
) -> APIResult {
    verify_admin(&auth)?;
    let inbound = inbound.into_inner();
    info!(
        "Preparing to delete format entitlement {:?} (requested by user ID {}).",
        inbound, auth.id
    );
    FormatEntitlementQuery::find_by_id(&db.conn, &inbound)
        .await?
        .ok_or_else(|| APIError::NotFound("format entitlement".into()))?
        .delete(&db.conn)
        .await?;
    HttpResponse::NoContent().finish().to_ok()
}

pub fn init_format_entitlement_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/entitlement")
        .wrap(AuthMiddleware)
        .service(delete_entitlement)
        .service(get_all_format)
        .service(create_entitlement);

    cfg.service(scope);
}
