use std::fmt::Debug;

use entity::error::DatabaseQueryError;
use sea_orm::strum::AsRefStr;
use thiserror::Error;

#[derive(Error, Debug, AsRefStr)]
pub enum CoreError {
    #[error("User '{0}' exceeded grant limit")]
    GrantError(String),
    // Pass through PoisonedError
    #[error("Poisoned mutex error")]
    PoisonError,
    #[error(transparent)]
    DatabaseQueryError(#[from] DatabaseQueryError),
}
