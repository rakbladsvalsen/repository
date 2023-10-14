use async_trait::async_trait;
use central_repository_dao::user::{Model as UserModel, UpdatableModel};

use crate::{auth::hashing::UserPassword, error::APIError};

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
        let current_span = tracing::Span::current();
        self.password = actix_web::web::block(move || {
            let _guard = current_span.enter();
            UserPassword::from(password).to_hash()
        })
        .await??;
        Ok(())
    }
}

#[async_trait]
impl DBPrepare for UpdatableModel {
    async fn prepare(&mut self) -> Result<(), APIError> {
        if self.password.is_none() {
            return Ok(());
        }
        let password = self.password.clone().unwrap();
        // perform expensive crypto operation in threadpool
        let current_span = tracing::Span::current();
        self.password = Some(
            actix_web::web::block(move || {
                let _guard = current_span.enter();
                UserPassword::from(password).to_hash()
            })
            .await??,
        );
        Ok(())
    }
}
