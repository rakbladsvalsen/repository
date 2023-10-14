use crate::{
    api_key::{create_api_key, delete_api_key, get_all_api_keys, update_api_key},
    auth::hashing::UserPassword,
    auth::jwt::Token,
    conf::{DB_POOL, PROTECT_SUPERUSER},
    core_middleware::auth::AuthMiddleware,
    error::{APIError, APIResponse, AsAPIResult},
    model_prepare::DBPrepare,
    pagination::{APIPager, PaginatedResponse},
    util::verify_admin,
};
use actix_web::{
    delete, get, patch, post,
    web::{self, Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{
    sea_orm::{ModelTrait, TryIntoModel},
    user::{Model as UserModel, ModelAsQuery, UpdatableModel},
    GetAllPaginated, UserMutation, UserQuery,
};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct LoginCredentials {
    pub username: String,
    pub password: String,
}

#[post("")]
async fn create_user(user: Json<UserModel>, auth: ReqData<UserModel>) -> APIResponse {
    verify_admin(&auth)?;
    let db = DB_POOL.get().expect("database is not initialized");
    let mut user = user.into_inner();
    let exists = UserQuery::find_by_username(db, &user.username).await?;
    if exists.is_some() {
        info!("Username '{}' already exists.", user.username);
        return APIError::DuplicateError.into();
    }
    // prepare this user for insert... (i.e. set password, etc).
    user.prepare().await?;
    HttpResponse::Created()
        .json(UserMutation::create(db, user).await?)
        .to_ok()
}

#[get("")]
async fn get_all_users(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    pager.validate()?;
    verify_admin(&auth)?;
    let filter = filter.into_inner();
    let pager = pager.into_inner().into();
    let users =
        UserQuery::get_all(db, &filter, &pager, None, entity::user::Column::CreatedAt).await?;
    Ok(PaginatedResponse::from(users).into())
}

#[post("")]
async fn login(inbound: Json<LoginCredentials>) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    let inbound = inbound.into_inner();
    let user = UserQuery::find_by_username(db, &inbound.username)
        .await?
        .ok_or(APIError::InvalidCredentials)?
        .try_into_model()?;
    if !user.active {
        info!("user {:?} (id={}) is inactive", user.username, user.id);
        return APIError::InactiveUser.into();
    }
    let current_span = tracing::Span::current();
    // don't block the main thread with crypto operations.
    Ok(web::block(move || {
        let _guard = current_span.enter();
        UserPassword::verify_password(&inbound.password, &user.password)
            .and_then(|_| Token::build_from_user(user))
    })
    .await??
    .into())
}

#[get("{id}")]
async fn get_user(id: Path<Uuid>, auth: ReqData<UserModel>) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    let id = id.into_inner();
    if !auth.is_superuser && auth.id != id {
        // don't allow non-superusers to view other users
        return Err(APIError::AdminOnlyResource);
    }
    let user = UserQuery::find_by_id(db, id)
        .await?
        .ok_or_else(|| APIError::NotFound(format!("user with ID {id}")))?;
    HttpResponse::Ok().json(user).to_ok()
}

#[delete("{id}")]
async fn delete_user(id: Path<Uuid>, auth: ReqData<UserModel>) -> APIResponse {
    verify_admin(&auth)?;
    let db = DB_POOL.get().expect("database is not initialized");
    let id = id.into_inner();
    if auth.id == id {
        return APIError::InvalidOperation("You can't delete yourself".into()).into();
    }
    let user = UserQuery::find_by_id(db, id)
        .await?
        .ok_or_else(|| APIError::NotFound(format!("user with ID {id}")))?;
    if *PROTECT_SUPERUSER && user.is_superuser {
        info!(
            "Prevented user deletionx: user ID {} tried to delete a superuser (ID: {})",
            auth.id, id
        );
        return APIError::ConflictingOperation("can't delete a superuser".into()).into();
    }
    info!(
        "Preparing to delete user ID {} (requested by user ID {}).",
        id, auth.id
    );
    user.delete(db).await?;
    HttpResponse::NoContent().finish().to_ok()
}

#[get("/self")]
async fn get_self(auth: ReqData<UserModel>) -> APIResponse {
    let user = auth.into_inner();
    HttpResponse::Ok().json(user).to_ok()
}

#[patch("{id}")]
async fn update_user(
    id: Path<Uuid>,
    user: Json<UpdatableModel>,
    auth: ReqData<UserModel>,
) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    if !auth.is_superuser && auth.id != *id {
        info!("non-superuser attempted to update another user");
        return APIError::InsufficientPermissions.into();
    }
    if !auth.is_superuser && (user.is_superuser.is_some() || user.active.is_some()) {
        info!("non-superuser attempted to update sensitive fields");
        return APIError::InsufficientPermissions.into();
    }
    let user_to_update = UserQuery::find_by_id(db, *id)
        .await?
        .ok_or_else(|| APIError::NotFound(format!("user with ID {id}")))?;
    if *PROTECT_SUPERUSER && user_to_update.is_superuser {
        return APIError::ConflictingOperation("can't modify a superuser".into()).into();
    }
    let user = UserMutation::update(db, user_to_update, user.into_inner()).await?;
    HttpResponse::Ok().json(user).to_ok()
}

#[get("")]
async fn healthcheck() -> APIResponse {
    info!("healthcheck ping");
    let response = json!({"status": "200"});
    Ok(HttpResponse::Ok().json(response))
}

pub fn init_user_routes(cfg: &mut web::ServiceConfig) {
    let login_scope = web::scope("/login").service(login);
    let health_scope = web::scope("/healthcheck").service(healthcheck);
    let user_scope = web::scope("/user")
        .wrap(AuthMiddleware)
        .service(get_all_api_keys)
        .service(get_self)
        .service(get_all_users)
        .service(create_user)
        .service(delete_user)
        .service(update_user)
        .service(get_user)
        .service(update_api_key)
        .service(create_api_key)
        .service(delete_api_key);

    cfg.service(health_scope);
    cfg.service(login_scope);
    cfg.service(user_scope);
}
