use crate::{
    auth::jwt::Token,
    conf::{DB_POOL, MAX_API_KEYS_PER_USER},
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{APIPager, PaginatedResponse},
    util::verify_admin,
};
use actix_web::{
    delete, get, patch, post,
    web::{Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_dao::{user::Model as UserModel, ApiKeyMutation, ApiKeyQuery};
use entity::api_key::{ModelAsQuery, UpdatableModel as ApiKeyUpdatableModel};
use itertools::Itertools;
use log::info;
use uuid::Uuid;

#[post("{user}/api-key")]
pub async fn create_api_key(user: Path<Uuid>, auth: ReqData<UserModel>) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    let user_id = user.into_inner();
    info!("api key: user: {:?}, target ID: {:?}", auth.id, user_id);
    if user_id != auth.id {
        // if this user is trying to create api key for someone else,
        // check if it has admin permissions.
        verify_admin(&auth)?;
    }
    if auth.is_superuser && user_id == auth.id {
        return Err(APIError::InvalidOperation(
            "cannot create api keys for an admin".into(),
        ));
    }

    let user = match ApiKeyQuery::get_user_and_keys(db, user_id).await? {
        Some((user, keys)) => {
            if keys.len() >= *MAX_API_KEYS_PER_USER {
                return Err(APIError::InvalidOperation(format!(
                    "Cannot have more than {} keys. Plase delete one of the following {} keys: {}",
                    *MAX_API_KEYS_PER_USER,
                    keys.len(),
                    keys.iter().map(|it| it.id).join(",")
                )));
            }
            user
        }
        _ => {
            return Err(APIError::NotFound(format!(
                "User ID '{}' does not exist.",
                user_id
            )))
        }
    };

    let api_key = ApiKeyMutation::create_for_user(db, &user).await?;
    let json = Token::create_api_key(user, api_key).await?;
    HttpResponse::Created().json(json).to_ok()
}

/// Update this api key.
/// {user} <- 1st item of user_and_key_id
/// {key_id} <- 2nd item of user_and_key_id
#[patch("{user}/api-key/{key_id}")]
pub async fn update_api_key(
    user_and_key_id: Path<(Uuid, Uuid)>,
    auth: ReqData<UserModel>,
    new: Json<ApiKeyUpdatableModel>,
) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    let (user_id, key_id) = user_and_key_id.into_inner();
    info!("api key: user: {:?}, target ID: {:?}", auth.id, user_id);
    if user_id != auth.id {
        // if this user is trying to create api key for someone else,
        // check if it has admin permissions.
        verify_admin(&auth)?;
    }
    if auth.is_superuser && user_id == auth.id {
        return Err(APIError::InvalidOperation(
            "cannot modify api keys for an admin".into(),
        ));
    }

    let key = match ApiKeyQuery::get_user_and_single_key(db, user_id, key_id).await? {
        Some((_user, key)) => key,
        _ => {
            return Err(APIError::NotFound(format!(
                "Cannot find key with id '{}' for user id '{}'",
                key_id, user_id
            )))
        }
    };

    let json = ApiKeyMutation::update(db, key, new.into_inner()).await?;
    HttpResponse::Ok().json(json).to_ok()
}

/// Update this api key.
/// {user} <- 1st item of user_and_key_id
/// {key_id} <- 2nd item of user_and_key_id
#[delete("{user}/api-key/{key_id}")]
pub async fn delete_api_key(
    user_and_key_id: Path<(Uuid, Uuid)>,
    auth: ReqData<UserModel>,
) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    let (user_id, key_id) = user_and_key_id.into_inner();
    info!("api key: user: {:?}, target ID: {:?}", auth.id, user_id);
    if user_id != auth.id {
        // if this user is trying to create api key for someone else,
        // check if it has admin permissions.
        verify_admin(&auth)?;
    }
    if auth.is_superuser && user_id == auth.id {
        return Err(APIError::InvalidOperation(
            "cannot modify api keys for an admin".into(),
        ));
    }

    let key = match ApiKeyQuery::get_user_and_single_key(db, user_id, key_id).await? {
        Some((_user, key)) => key,
        _ => {
            return Err(APIError::NotFound(format!(
                "Cannot find key with id '{}' for user id '{}'",
                key_id, user_id
            )))
        }
    };
    ApiKeyMutation::delete(db, key).await?;
    HttpResponse::NoContent().finish().to_ok()
}

#[get("api-key")]
async fn get_all_api_keys(
    pager: Query<APIPager>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
) -> APIResponse {
    let db = DB_POOL.get().expect("database is not initialized");
    pager.validate()?;
    let filter = filter.into_inner();
    let pager = pager.into_inner().into();
    let user = auth.into_inner();
    let entries = ApiKeyQuery::get_all_for_user(db, &filter, &pager, user).await?;
    Ok(PaginatedResponse::from(entries).into())
}
