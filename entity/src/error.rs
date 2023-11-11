use std::fmt::Debug;

use sea_orm::DbErr;
use strum::AsRefStr;
use thiserror::Error;

#[derive(Error, Debug, AsRefStr)]
pub enum DatabaseQueryError {
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    #[error("Column '{0}' doesn't exist in any of the requested/available formats")]
    InvalidColumnRequested(String),
    #[error("Invalid usage: {0}")]
    InvalidUsage(String),
    #[error("Couldn't cast value to expected type")]
    CastError,
    #[error("One or more formats have different types for column: '{0}'")]
    ColumnWithMixedTypesError(String),
    #[error("Empty query")]
    EmptyQuery,
    #[error("Regex error")]
    InvalidRegex,
    #[error("Internal DB error: {0}")]
    DbErr(#[from] DbErr),
}
