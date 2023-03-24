use actix_example_core::user::Model as UserModel;
use async_trait::async_trait;

use crate::{auth::password::UserPassword, error::APIError};

#[async_trait]
pub trait DBPrepare {
    /// Prepare any given object for database insertion.
    async fn prepare(&mut self) -> Result<(), APIError>;
}

#[async_trait]
impl DBPrepare for UserModel {
    async fn prepare(&mut self) -> Result<(), APIError> {
        let password = self.password.clone();
        // perform expensive crypto operation in threadpool
        self.password =
            actix_web::web::block(move || UserPassword::from(password).try_into()).await??;
        Ok(())
    }
}
