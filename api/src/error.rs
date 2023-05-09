use actix_web::{
    error::{self, BlockingError},
    http::StatusCode,
    web::{self, JsonConfig, PathConfig, QueryConfig},
    HttpResponse,
};
use central_repository_dao::error::DatabaseQueryError;
use log::{error, info};
use sea_orm::{DbErr, RuntimeErr};
use serde::Serialize;
use sqlx::Error as SQLXError;
use strum::AsRefStr;

use thiserror::Error;

use crate::{common::handle_fatal, conf::MAX_JSON_PAYLOAD_SIZE};

pub type APIResult = Result<HttpResponse, APIError>;

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

#[derive(Error, Debug, Serialize, AsRefStr, Clone)]
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
}

impl APIError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::DuplicateError => StatusCode::BAD_REQUEST,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::ValidationFailure(_) => StatusCode::BAD_REQUEST,
            Self::ServerError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidCredentials => StatusCode::UNAUTHORIZED,
            Self::InvalidToken => StatusCode::UNAUTHORIZED,
            Self::MissingAuthHeader => StatusCode::UNAUTHORIZED,
            Self::AdminOnlyResource => StatusCode::FORBIDDEN,
            Self::InsufficientPermissions => StatusCode::FORBIDDEN,
            Self::InvalidOperation(_) => StatusCode::BAD_REQUEST,
            Self::ConflictingOperation(_) => StatusCode::CONFLICT,
            Self::InvalidQuery(_) => StatusCode::BAD_REQUEST,
            Self::CastError(_, _) => StatusCode::BAD_REQUEST,
            Self::InvalidPageSize(_) => StatusCode::BAD_REQUEST,
            Self::InactiveUser => StatusCode::UNAUTHORIZED,
        }
    }
}

impl From<DatabaseQueryError> for APIError {
    fn from(value: DatabaseQueryError) -> Self {
        match value {
            // this is just a regular seaorm DbErr.
            DatabaseQueryError::NestedDBError(err) => APIError::from(err),
            DatabaseQueryError::ColumnWithMixedTypesError(_) => {
                APIError::InvalidQuery(value.to_string())
            }
            DatabaseQueryError::InvalidUsage(_) => APIError::InvalidQuery(value.to_string()),
            _ => APIError::InvalidQuery(value.to_string()),
        }
    }
}

impl From<DbErr> for APIError {
    fn from(error: DbErr) -> APIError {
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
                    return DatabaseQueryError::InvalidUsage("malformed regular expression".into())
                        .into();
                }
                handle_fatal!("SqlxError", err, APIError::ServerError)
            }
            DbErr::RecordNotFound(err) => APIError::NotFound(err),
            unhandled => handle_fatal!("unhandled db error", unhandled, APIError::ServerError),
        }
    }
}

impl From<BlockingError> for APIError {
    fn from(error: BlockingError) -> APIError {
        handle_fatal!("blocking error", error, APIError::ServerError)
    }
}

impl error::ResponseError for APIError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self) -> HttpResponse {
        info!("APIError: {:#?}", self);
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

impl From<APIError> for APIResult {
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
    fn to_ok(self) -> APIResult;
}

impl AsAPIResult for HttpResponse {
    /// This basically converts an HTTPResponse into
    /// APIResult. It is not possible to implement
    /// From<HttpResponse> for APIResult, so we're using
    /// this as a workaround.
    fn to_ok(self) -> APIResult {
        Ok(self)
    }
}
