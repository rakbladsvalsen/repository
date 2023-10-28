use std::fmt::Debug;

use entity::error::DatabaseQueryError;
use sea_orm::strum::AsRefStr;
use thiserror::Error;

#[derive(Error, Debug, AsRefStr)]
pub enum CoreError {
    #[error("User '{0}' already holds {1} grant(s)")]
    GrantError(String, u64),
    // Pass through PoisonedError
    #[error("Poisoned mutex error")]
    PoisonError,
    #[error(transparent)]
    DatabaseQueryError(#[from] DatabaseQueryError),
}
