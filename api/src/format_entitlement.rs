use crate::{
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{PaginatedResponse, Validate},
    util::verify_admin,
};
use actix_web::{
    delete, get, post, web,
    web::{Json, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    conf::DBConfig,
    format_entitlement::{ModelAsQuery, SearchModel as FormatEntitlementSearch},
    sea_orm::ModelTrait,
    user::Model,
    FormatEntitlementMutation, FormatEntitlementQuery, FormatQuery, GetAllPaginated,
    PaginationOptions, UserQuery,
};
use entity::format_entitlement::Model as FormatEntitlementModel;
use log::info;

#[post("")]
async fn create_entitlement(
    inbound: Json<FormatEntitlementModel>,
    auth: ReqData<Model>,
) -> APIResponse {
    verify_admin(&auth)?;

    if inbound.access.is_empty() {
        return Err(APIError::BadRequest);
    }
    // make sure we're assigning a format to a non-superuser
    UserQuery::find_nonsuperuser_by_id(inbound.user_id)
        .await?
        .ok_or_else(|| {
            info!("Couldn't find user id {}", inbound.user_id);
            APIError::NotFound(format!("non-superuser with ID {}", inbound.user_id))
        })?;
    // make sure this format exists before creating the entitlement
    FormatQuery::find_by_id(&auth, inbound.format_id)
        .await?
        .ok_or_else(|| {
            info!("Couldn't find format id {}", inbound.format_id);
            APIError::NotFound(format!("format with ID {}", inbound.format_id))
        })?;
    HttpResponse::Created()
        .json(FormatEntitlementMutation::create(inbound.into_inner()).await?)
        .to_ok()
}

#[get("")]
async fn get_all_entitlements(
    pager: Query<PaginationOptions>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<Model>,
) -> APIResponse {
    pager.validate()?;
    let auth = auth.into_inner();
    let filter = filter.into_inner();
    let pager = pager.into_inner();
    Ok(PaginatedResponse::from(
        FormatEntitlementQuery::get_all_filtered_for_user(&filter, &pager, auth, None).await?,
    )
    .into())
}

#[delete("")]
async fn delete_entitlement(
    inbound: Json<FormatEntitlementSearch>,
    auth: ReqData<Model>,
) -> APIResponse {
    verify_admin(&auth)?;
    let inbound = inbound.into_inner();
    info!(
        "Preparing to delete format entitlement {:?} (requested by user ID {}).",
        inbound, auth.id
    );
    FormatEntitlementQuery::find_by_id(&inbound)
        .await?
        .ok_or_else(|| APIError::NotFound("format entitlement".into()))?
        .delete(DBConfig::get_connection())
        .await?;
    HttpResponse::NoContent().finish().to_ok()
}

pub fn init_format_entitlement_routes(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/entitlement")
        .wrap(AuthMiddleware)
        .service(delete_entitlement)
        .service(get_all_entitlements)
        .service(create_entitlement);

    cfg.service(scope);
}
