use crate::{
    auth::jwt::Token,
    auth::password::UserPassword,
    common::AppState,
    conf::PROTECT_SUPERUSER,
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResult, AsAPIResult},
    model_prepare::DBPrepare,
    pagination::{APIPager, PaginatedResponse},
    util::verify_admin,
};
use actix_web::{
    delete, get, post,
    web::{self, Data, Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    sea_orm::{ModelTrait, TryIntoModel},
    user::{Model as UserModel, ModelAsQuery},
    GetAllPaginated, UserMutation, UserQuery,
};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
}

#[post("")]
async fn create_user(
    user: Json<UserModel>,
    db: Data<AppState>,
    auth: ReqData<UserModel>,
) -> APIResult {
    verify_admin(&auth)?;
    let mut user = user.into_inner();
    let exists = UserQuery::find_by_username(&db.conn, &user.username).await?;
    if exists.is_some() {
        info!("Username '{}' already exists.", user.username);
        return APIError::DuplicateError.into();
    }
    // prepare this user for insert... (i.e. set password, etc).
    user.prepare().await?;
    HttpResponse::Created()
        .json(UserMutation::create(&db.conn, user).await?)
        .to_ok()
}

#[get("")]
async fn get_all_users(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    db: web::Data<AppState>,
    auth: ReqData<UserModel>,
) -> APIResult {
    pager.validate()?;
    verify_admin(&auth)?;
    let filter = filter.into_inner();
    let users = UserQuery::get_all(&db.conn, &filter, pager.page, pager.per_page, None).await?;
    Ok(PaginatedResponse::from(users).into())
}

#[post("")]
async fn login(inbound: Json<LoginCredentials>, db: Data<AppState>) -> APIResult {
    let inbound = inbound.into_inner();
    let user = UserQuery::find_by_username(&db.conn, &inbound.username)
        .await?
        .ok_or(APIError::InvalidCredentials)?
        .try_into_model()?;
    info!(
        "Trying to authenticate user {}/{:?}.",
        user.id, user.username
    );
    let current_span = tracing::Span::current();
    // don't block the main thread with crypto operations.
    Ok(web::block(move || {
        let _guard = current_span.enter();
        UserPassword::verify_password(&inbound.password, &user.password)
            .and_then(|_| Token::build(user.id))
    })
    .await??
    .into())
}

#[delete("{id}")]
async fn delete_user(
    id: Option<Path<i32>>,
    db: Data<AppState>,
    auth: ReqData<UserModel>,
) -> APIResult {
    verify_admin(&auth)?;
    let id = id.ok_or(APIError::BadRequest)?.into_inner();
    if auth.id == id {
        return APIError::InvalidOperation("You can't delete yourself".into()).into();
    }
    let user = UserQuery::find_by_id(&db.conn, id)
        .await?
        .ok_or_else(|| APIError::NotFound(format!("user with ID {id}")))?;
    if !*PROTECT_SUPERUSER && user.is_superuser {
        info!(
            "Prevented user deletion: user ID {} tried to delete a superuser (ID: {})",
            auth.id, id
        );
        return APIError::ConflictingOperation("can't delete a superuser".into()).into();
    }
    info!(
        "Preparing to delete user ID {} (requested by user ID {}).",
        id, auth.id
    );
    user.delete(&db.conn).await?;
    HttpResponse::NoContent().finish().to_ok()
}

#[post("/token/validate")]
async fn validate_token(token: Json<Token>, db: Data<AppState>) -> APIResult {
    let user = token.into_inner().validate(&db.conn).await?;
    HttpResponse::Ok().json(user).to_ok()
}

#[get("")]
async fn healthcheck() -> APIResult {
    info!("healthcheck ping");
    let response = json!({"status": "200"});
    Ok(HttpResponse::Ok().json(response))
}

pub fn init_user_routes(cfg: &mut web::ServiceConfig) {
    let login_scope = web::scope("/login").service(login);
    let health_scope = web::scope("/healthcheck").service(healthcheck);
    let user_scope = web::scope("/user")
        .wrap(AuthMiddleware)
        .service(get_all_users)
        .service(create_user)
        .service(delete_user)
        .service(validate_token);
    cfg.service(health_scope);
    cfg.service(login_scope);
    cfg.service(user_scope);
}
