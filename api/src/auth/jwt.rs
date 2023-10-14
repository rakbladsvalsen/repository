use crate::{
    common::handle_fatal,
    conf::{CONFIG, DECODING_KEY, ENCODING_KEY},
};
use actix_web::{web, HttpResponse};
use argon2::Argon2;
use central_repository_dao::{sea_orm::DbConn, user::Model as UserModel, ApiKeyQuery, UserQuery};
use chrono::{Duration, Utc};
use entity::api_key::Model as ApiKeyModel;
use jsonwebtoken::{decode, encode, Algorithm, Header, Validation};
use lazy_static::lazy_static;
use log::{error, info, warn};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::APIError;

use super::hashing::StringHashUtil;

lazy_static! {
    static ref JWT_HEADER: Header = Header::new(Algorithm::EdDSA);
    pub static ref ARGON: Argon2<'static> = Argon2::default();

    static ref VALIDATION: Validation = {
        let mut ret = Validation::new(Algorithm::EdDSA);
        // disable clock skew leeway
        ret.leeway = 0;
        ret
    };
}

#[derive(Debug, Deserialize)]
pub struct Token {
    token: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    token: String,
    user: UserModel,

    // Only applies to API keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<ApiKeyModel>,
}

impl Token {
    #[inline(always)]
    pub async fn gen_rand_secret_with_hash(sample: usize) -> Result<(String, String), APIError> {
        web::block(move || {
            let original = String::new_random(sample);
            let hashed = original.try_get_argon_hash()?;
            Ok::<_, APIError>((original, hashed))
        })
        .await?
    }

    /// Creates an API Token for the user `user`.
    /// This function creates longer-lived tokens
    pub async fn create_api_key(
        user: UserModel,
        api_key: ApiKeyModel,
    ) -> Result<TokenResponse, APIError> {
        let mut claims = Claims::new_long_lived(&user);
        // Only the user should have the secret (`original`).
        claims.aks = Some(ApiKeyData {
            id: api_key.id,
            lra: api_key.last_rotated_at.timestamp() as usize,
        });

        Ok(TokenResponse {
            user,
            token: claims.try_to_jwt()?,
            api_key: Some(api_key),
        })
    }

    /// Creates a short-lived token for a given user.
    /// This function only forges short-lived tokens. Longer-lived tokens
    /// should be generated using the API key function.
    pub fn build_from_user(user: UserModel) -> Result<TokenResponse, APIError> {
        Ok(TokenResponse {
            token: Claims::new_short_lived(&user).try_to_jwt()?,
            user,
            api_key: None,
        })
    }

    /// Validate API key.
    #[inline(always)]
    async fn validate_api_key(db: &DbConn, token: Claims) -> Result<UserModel, APIError> {
        let api_key_data = token.aks.as_ref().ok_or(APIError::ServerError)?;
        let user_and_key =
            ApiKeyQuery::get_user_and_single_key(db, token.sub, api_key_data.id).await?;

        let (user, key) = match user_and_key {
            Some((user, key)) => (user, key),
            _ => return Err(APIError::InvalidToken),
        };

        if !key.active {
            info!(
                "Received a valid token (user id: {}, token id: {}) but key is disabled",
                user.id, key.id
            );
            return Err(APIError::InactiveKey);
        }

        if !user.active {
            info!(
                "Received a valid token but user (id: {}) is disabled",
                user.id
            );
            return Err(APIError::InactiveUser);
        }

        if api_key_data.lra != key.last_rotated_at.timestamp() as usize {
            info!(
                "Token was rotated (user id: {}, token id: {})",
                user.id, key.id
            );
            return Err(APIError::InvalidToken);
        }

        info!(
            "successfully validated API token for user: {}: '{}'",
            user.id, user.username
        );
        Ok(user)
    }

    /// Validate user token.
    #[inline(always)]
    async fn validate_user_token(db: &DbConn, token: Claims) -> Result<UserModel, APIError> {
        let user = UserQuery::find_by_id(db, token.sub).await?.ok_or_else(|| {
            warn!(
                "Received a valid token but user was deleted (id {}).",
                token.sub
            );
            APIError::InvalidToken
        })?;
        if !user.active {
            info!(
                "Received a valid token but user (id: {}) is disabled.",
                user.id
            );
            return Err(APIError::InactiveUser);
        }
        Ok(user)
    }

    /// Validates a JWT token. Returns an instance of the user on success.
    #[inline(always)]
    pub async fn validate(&self, db: &DbConn) -> Result<UserModel, APIError> {
        // try to decode and validate token data.
        let token = Claims::try_from_jwt(&self.token)?;
        // token is valid, now validate the user (and the token)
        if token.aks.is_some() {
            return Self::validate_api_key(db, token).await;
        }
        Self::validate_user_token(db, token).await
    }
}

impl From<String> for Token {
    fn from(value: String) -> Self {
        Token { token: value }
    }
}

impl From<TokenResponse> for HttpResponse {
    fn from(value: TokenResponse) -> Self {
        HttpResponse::Ok().json(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiKeyData {
    // id:
    // This API Key's ID in the database.
    id: Uuid,
    // The last time this API key was rotated.
    // This is used to determine whether the key has been rotated
    // (and thus, invalidated) since the last time it was issued.
    lra: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    // subject - user id
    sub: Uuid,
    // subject - username (user)
    user: String,
    // superuser status
    su: bool,
    // issued_at
    iat: usize,
    // expires_at
    exp: usize,

    // ApiKey-only attributes
    #[serde(skip_serializing_if = "Option::is_none")]
    aks: Option<ApiKeyData>,
}

impl Claims {
    /// Try to decode a Claim from a JWT.
    #[inline(always)]
    fn try_from_jwt(jwt: &str) -> Result<Claims, APIError> {
        let token = decode::<Claims>(jwt, &DECODING_KEY, &VALIDATION).map_err(|err| {
            info!("Token validation failure: {:?}", err);
            APIError::InvalidToken
        })?;
        Ok(token.claims)
    }

    /// Convert this claim to an encoded JWT.
    #[inline(always)]
    fn try_to_jwt(&self) -> Result<String, APIError> {
        encode(&JWT_HEADER, &self, &ENCODING_KEY).map_err(|err| {
            // crypto/memory error
            handle_fatal!("token creation", err, APIError::ServerError)
        })
    }

    #[inline(always)]
    fn new_from_user(user: &UserModel, expires_in: Duration) -> Self {
        let now = Utc::now();
        Claims {
            sub: user.id,
            user: user.username.clone(),
            su: user.is_superuser,
            iat: now.timestamp() as usize,
            exp: (now + expires_in).timestamp() as usize,
            aks: None,
        }
    }

    /// Generate a new short-lived token using the default duration (token
    /// expiration seconds).
    #[inline(always)]
    fn new_short_lived(user: &UserModel) -> Self {
        Self::new_from_user(
            user,
            Duration::seconds(CONFIG.token_expiration_seconds.into()),
        )
    }

    /// Generate a long-lived token using the api key duration.
    #[inline(always)]
    fn new_long_lived(user: &UserModel) -> Self {
        Self::new_from_user(
            user,
            Duration::hours(CONFIG.token_api_key_expiration_hours as i64),
        )
    }
}
