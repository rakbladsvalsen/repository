use actix_web::{
    error::{self, BlockingError},
    http::StatusCode,
    web::{self, JsonConfig, PathConfig, QueryConfig},
    HttpResponse,
};
use central_repository_dao::CoreError;
use entity::error::DatabaseQueryError;
use log::{error, info};
use sea_orm::{DbErr, RuntimeErr};
use serde::Serialize;
use sqlx::Error as SQLXError;
use strum::AsRefStr;

use thiserror::Error;

use crate::{common::handle_fatal, conf::MAX_JSON_PAYLOAD_SIZE};

pub type APIResult<T> = Result<T, APIError>;

pub type APIResponse = APIResult<HttpResponse>;

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutboundAPIError {
    pub status_code: u16,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Error, Debug, Serialize, Clone, Copy)]
pub enum ValidationFailureKind {
    #[error("One or more fields are invalid")]
    InvalidRequestData,
    #[error("One or more fields contain data with mistmatched types")]
    MismatchedDataType,
    #[error("One or more entries either lack or contain invalid/unknown keys")]
    MissingDictKeys,
    #[error("Only strings and numbers are supported")]
    InvalidComparisonKind,
    #[error("Regex match failure: data doesn't match regex")]
    RegexMatchFailure,
}

#[derive(Error, Debug, AsRefStr)]
pub enum APIError {
    #[error("An item with similar data already exists.")]
    DuplicateError,
    #[error("Bad request.")]
    BadRequest,
    #[error("Validation error: {0}.")]
    ValidationFailure(ValidationFailureKind),
    #[error("Server error.")]
    ServerError,
    #[error("Couldn't find {0}.")]
    NotFound(String),
    #[error("Cannot authenticate: user is inactive")]
    InactiveUser,
    #[error("Cannot authenticate: key is inactive")]
    InactiveKey,
    #[error("Invalid credentials.")]
    InvalidCredentials,
    #[error("Invalid or expired token.")]
    InvalidToken,
    #[error("Missing 'Authentication' header.")]
    MissingAuthHeader,
    #[error("Insufficient permissions: only admins may use this resource.")]
    AdminOnlyResource,
    #[error("Insufficient permissions: you need one or more roles to access this resource.")]
    InsufficientPermissions,
    #[error("Invalid operation: {0}.")]
    InvalidOperation(String),
    #[error("Conflicting operation: {0}.")]
    ConflictingOperation(String),
    #[error("Invalid data type: cannot cast {0} to type {1}")]
    CastError(String, String),
    #[error("Query error: {0}")]
    InvalidQuery(String),
    #[error("Invalid pagination size: {0}")]
    InvalidPageSize(String),
    #[error("Fatal threading error")]
    BlockingError(#[from] BlockingError),
    #[error("Rate limit: {0}")]
    RateLimit(String),
}

impl APIError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::DuplicateError | Self::BadRequest | Self::ValidationFailure(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound(_)
            | Self::InvalidCredentials
            | Self::InvalidToken
            | Self::MissingAuthHeader => StatusCode::UNAUTHORIZED,
            Self::AdminOnlyResource | Self::InsufficientPermissions => StatusCode::FORBIDDEN,
            Self::InvalidOperation(_)
            | Self::ConflictingOperation(_)
            | Self::InvalidQuery(_)
            | Self::CastError(_, _)
            | Self::InvalidPageSize(_) => StatusCode::BAD_REQUEST,
            Self::InactiveUser | Self::InactiveKey => StatusCode::UNAUTHORIZED,
            Self::BlockingError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::RateLimit(_) => StatusCode::TOO_MANY_REQUESTS,
        }
    }

    /// Match special database-level errors (aka DbErr)
    /// This function makes sure we're not leaking any information to end users.
    fn from_db_err(error: &DbErr) -> APIError {
        match error {
            DbErr::Query(RuntimeErr::SqlxError(SQLXError::Database(err))) => {
                // SQLX::Database errors don't have any enums inside, so there's
                // no other way to know what the error was. This "duplicate key value" is something
                // that only works with postgres.
                let err_as_string = err.to_string().to_lowercase();
                if err_as_string.contains("duplicate key value") {
                    info!("Caught duplicate key error: {}", err);
                    return APIError::DuplicateError;
                } else if err_as_string.contains("invalid regular expression") {
                    info!("got invalid regular expression");
                    return APIError::InvalidQuery("regex is malformed".into());
                }
                handle_fatal!("SqlxError", err, APIError::ServerError)
            }
            DbErr::RecordNotFound(err) => APIError::NotFound(err.to_string()),
            unhandled => handle_fatal!("unhandled db error", unhandled, APIError::ServerError),
        }
    }
}

impl From<DbErr> for APIError {
    #[inline(always)]
    fn from(error: DbErr) -> APIError {
        APIError::from_db_err(&error)
    }
}

impl From<DatabaseQueryError> for APIError {
    #[inline(always)]
    fn from(value: DatabaseQueryError) -> Self {
        APIError::from(&value)
    }
}

impl From<&DatabaseQueryError> for APIError {
    #[inline(always)]
    fn from(value: &DatabaseQueryError) -> Self {
        match value {
            // this is just a regular seaorm DbErr.
            DatabaseQueryError::DbErr(err) => APIError::from_db_err(err),
            _ => APIError::InvalidQuery(value.to_string()),
        }
    }
}

impl From<CoreError> for APIError {
    #[inline(always)]
    fn from(value: CoreError) -> Self {
        match value {
            CoreError::GrantError(msg) => APIError::RateLimit(msg),
            CoreError::PoisonError => APIError::ServerError,
            // CoreError can also have DatabaseQueryError's inside. In this case,
            // we just delegate the conversion.
            CoreError::DatabaseQueryError(e) => APIError::from(e),
        }
    }
}

impl error::ResponseError for APIError {
    #[inline(always)]
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    #[inline(always)]
    fn error_response(&self) -> HttpResponse {
        let out = OutboundAPIError {
            status_code: u16::from(self.status_code()),
            detail: Some(self.to_string()),
            kind: self.as_ref().into(),
        };
        let mut response = HttpResponse::build(self.status_code());
        if self.status_code() == StatusCode::UNAUTHORIZED {
            response.insert_header(("WWW-Authenticate", "Bearer"));
        }
        response.json(out)
    }
}

impl From<APIError> for APIResponse {
    #[inline(always)]
    fn from(value: APIError) -> Self {
        Err(value)
    }
}

pub fn json_error_handler() -> JsonConfig {
    web::JsonConfig::default()
        // limit request payload size
        .limit(*MAX_JSON_PAYLOAD_SIZE)
        .error_handler(|err, _| {
            info!("JSON deserialization error: {:?}", err);
            APIError::BadRequest.into()
        })
}

pub fn query_error_handler() -> QueryConfig {
    web::QueryConfig::default().error_handler(|err, _| {
        info!("Query param deserialization error: {:?}", err);
        APIError::BadRequest.into()
    })
}

pub fn path_error_handler() -> PathConfig {
    PathConfig::default().error_handler(|err, _| {
        info!("Path param deserialization error: {:?}", err);
        APIError::BadRequest.into()
    })
}

pub trait AsAPIResult {
    fn to_ok(self) -> APIResponse;
}

impl AsAPIResult for HttpResponse {
    /// This basically converts an HTTPResponse into
    /// APIResult. It is not possible to implement
    /// From<HttpResponse> for APIResult, so we're using
    /// this as a workaround.
    fn to_ok(self) -> APIResponse {
        Ok(self)
    }
}
