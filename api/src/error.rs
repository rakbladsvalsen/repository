use std::error::Error;

use actix_web::{
    error::{self, BlockingError},
    http::StatusCode,
    HttpResponse,
};
use central_repository_dao::sea_orm::{strum::AsRefStr, RuntimeErr};
use log::{error, info};
use migration::DbErr;
use serde::Serialize;
use sqlx::Error as SQLXError;

use thiserror::Error;
use validator::ValidationErrors;

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
        }
    }
}

fn handle_fatal(err: Box<dyn Error>) -> APIError {
    error!("Unhandled error: {:?}: {}", err, err.to_string());
    APIError::ServerError
}

impl From<DbErr> for APIError {
    fn from(error: DbErr) -> APIError {
        match error {
            DbErr::Query(RuntimeErr::SqlxError(SQLXError::Database(err))) => {
                // SQLX::Database errors don't have any enums inside, so there's
                // no other way to know what the error was. This only works with
                // vanilla postgres.
                if err.to_string().contains("duplicate key value") {
                    info!("Caught duplicate key error: {}", err);
                    return APIError::DuplicateError;
                }
                error!("Unhandled SqlxError::Database error: {}", err);
                APIError::BadRequest
            }
            DbErr::RecordNotFound(err) => APIError::NotFound(err),
            // DbErr::Query(error) => handle_fatal(error.into()),
            // DbErr::AttrNotSet(error) => handle_fatal(error.into()),
            // handle any other DbErr
            unhandled => handle_fatal(unhandled.into()),
        }
    }
}

impl From<BlockingError> for APIError {
    fn from(error: BlockingError) -> APIError {
        handle_fatal(error.into())
    }
}

impl From<ValidationErrors> for APIError {
    // transform `validator`'s ValidationErrors into
    // a proper APIError
    fn from(error: ValidationErrors) -> APIError {
        info!("Caught ValidationError: {:?}", error);
        // let errors = error
        //     .errors()
        //     .keys()
        //     .into_iter()
        //     // Grab all fields with errors and concatenate them in a
        //     // nice, readable string, i.e. 'Field1', 'Field2', etc.
        //     .map(|i| format!("'{}'", *i))
        //     .collect::<Vec<_>>()
        //     .join(",");
        APIError::ValidationFailure(ValidationFailureKind::InvalidRequestData)
    }
}

impl error::ResponseError for APIError {
    fn error_response(&self) -> HttpResponse {
        // transform APIError(s) into Actix HTTPResponse
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
