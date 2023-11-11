use crate::{
    auth::jwt::Token,
    error::{APIError, APIResponse, AsAPIResult},
    pagination::{PaginatedResponse, Validate},
    util::verify_admin,
};
use actix_web::{
    delete, get, patch, post,
    web::{Json, Path, Query, ReqData},
    HttpResponse,
};
use central_repository_config::inner::Config;
use central_repository_dao::{
    user::Model as UserModel, ApiKeyMutation, ApiKeyQuery, GetAllPaginated, PaginationOptions,
};
use entity::api_key::{ModelAsQuery, UpdatableModel as ApiKeyUpdatableModel};
use itertools::Itertools;
use log::info;
use uuid::Uuid;

#[post("{user}/api-key")]
pub async fn create_api_key(user: Path<Uuid>, auth: ReqData<UserModel>) -> APIResponse {
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

    let user = match ApiKeyQuery::get_user_and_keys(user_id).await? {
        Some((user, keys)) => {
            if keys.len() >= Config::get().max_api_keys_per_user as usize {
                return Err(APIError::InvalidOperation(format!(
                    "Cannot have more than {} keys. Plase delete one of the following {} keys: {}",
                    Config::get().max_api_keys_per_user,
                    keys.len(),
                    keys.iter().map(|it| it.id).join(",")
                )));
            }
            user
        }
        _ => return Err(APIError::NotFound(format!("user ID '{}'", user_id))),
    };

    let api_key = ApiKeyMutation::create_for_user(&user).await?;
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

    let (user, key) = match ApiKeyQuery::get_user_and_single_key(user_id, key_id).await? {
        Some((user, key)) => (user, key),
        _ => {
            return Err(APIError::NotFound(format!(
                "key with id '{}' for user id '{}'",
                key_id, user_id
            )))
        }
    };

    let rotate_requested = new.rotate.unwrap_or_default();
    let api_key = ApiKeyMutation::update(key, new.into_inner()).await?;
    if !rotate_requested {
        // no need to forge token again since it wasn't rotated.
        return HttpResponse::Ok().json(api_key).to_ok();
    }
    // If we're updating the token, then forge it using the last known rotation time.
    let json = Token::create_api_key(user, api_key).await?;
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

    let key = match ApiKeyQuery::get_user_and_single_key(user_id, key_id).await? {
        Some((_user, key)) => key,
        _ => {
            return Err(APIError::NotFound(format!(
                "key with id '{}' for user id '{}'",
                key_id, user_id
            )))
        }
    };
    ApiKeyMutation::delete(key).await?;
    HttpResponse::NoContent().finish().to_ok()
}

#[get("api-key")]
async fn get_all_api_keys(
    pager: Query<PaginationOptions>,
    filter: Query<ModelAsQuery>,
    auth: ReqData<UserModel>,
) -> APIResponse {
    pager.validate()?;
    let filter = filter.into_inner();
    // todo!()
    let pager = pager.into_inner();
    let user = auth.into_inner();
    let entries = ApiKeyQuery::get_all_filtered_for_user(&filter, &pager, user, None).await?;
    Ok(PaginatedResponse::from(entries).into())
}
