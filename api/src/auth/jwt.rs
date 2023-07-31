use crate::{
    common::handle_fatal,
    conf::{CONFIG, DECODING_KEY, ENCODING_KEY},
};
use actix_web::HttpResponse;
use central_repository_dao::{sea_orm::DbConn, user::Model as UserModel, UserQuery};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, Header, Validation};
use lazy_static::lazy_static;
use log::{error, info, warn};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::APIError;

lazy_static! {
    static ref JWT_HEADER: Header = Header::new(Algorithm::EdDSA);
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
pub struct TokenResponse {
    token: String,
    user: UserModel,
}

impl Token {
    pub fn build(user: UserModel) -> Result<TokenResponse, APIError> {
        let claims = Claims::new(&user);
        let token = encode(&JWT_HEADER, &claims, &ENCODING_KEY).map_err(|err| {
            // crypto/memory error
            handle_fatal!("token creation", err, APIError::ServerError)
        })?;
        Ok(TokenResponse { token, user })
    }

    /// Validates a JWT token. Returns an instance of the user on success.
    pub async fn validate(&self, db: &DbConn) -> Result<UserModel, APIError> {
        // try to decode and validate token data.
        let token_data =
            decode::<Claims>(&self.token, &DECODING_KEY, &VALIDATION).map_err(|err| {
                info!("Token validation failure: {:?}", err);
                APIError::InvalidToken
            })?;
        // make sure this user exists
        let user = UserQuery::find_by_id(db, token_data.claims.sub)
            .await?
            .ok_or_else(|| {
                warn!(
                    "Received a valid token but user was deleted (id {}).",
                    token_data.claims.sub
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
pub struct Claims {
    // subject - user id
    sub: Uuid,
    // subject - username (user)
    user: String,
    // subject - superuser-ness
    su: bool,
    // issued_at
    iat: usize,
    // expires_at
    exp: usize,
}

impl Claims {
    pub fn new(user: &UserModel) -> Self {
        let now = Utc::now();
        Claims {
            sub: user.id,
            user: user.username.clone(),
            su: user.is_superuser,
            iat: now.timestamp() as usize,
            exp: (now + Duration::seconds(CONFIG.token_expiration_seconds.into())).timestamp()
                as usize,
        }
    }
}
