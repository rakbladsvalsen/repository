use crate::conf::{CONFIG, DECODING_KEY, ENCODING_KEY};
use actix_example_core::{sea_orm::DbConn, user::Model as UserModel, UserQuery};
use actix_web::HttpResponse;
use chrono::Duration;
use jsonwebtoken::{decode, encode, Algorithm, Header, Validation};
use lazy_static::lazy_static;
use log::{error, info, warn};

use serde::{Deserialize, Serialize};
use sqlx::types::chrono::Utc;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Token {
    token: String,
}

impl Token {
    pub fn build(sub: i32) -> Result<Self, APIError> {
        let claims = Claims::new(sub);
        let token = encode(&JWT_HEADER, &claims, &ENCODING_KEY).map_err(|err| {
            // crypto/memory error
            error!("Couldn't build token (caused by: {:?})", err);
            APIError::ServerError
        })?;
        Ok(Token { token })
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
        UserQuery::find_by_id(db, token_data.claims.sub)
            .await?
            .ok_or_else(|| {
                warn!(
                    "Received a valid token but user was deleted (id {}).",
                    token_data.claims.sub
                );
                APIError::InvalidToken
            })
    }
}

impl From<String> for Token {
    fn from(value: String) -> Self {
        Token { token: value }
    }
}

impl From<Token> for HttpResponse {
    fn from(value: Token) -> Self {
        HttpResponse::Ok().json(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    // subject - user id
    sub: i32,
    // issued_at
    iat: usize,
    // expires_at
    exp: usize,
}

impl Claims {
    pub fn new(sub: i32) -> Self {
        let now = Utc::now();
        Claims {
            sub,
            iat: now.timestamp() as usize,
            exp: (now + Duration::seconds(CONFIG.token_expiration_seconds.into())).timestamp()
                as usize,
        }
    }
}
