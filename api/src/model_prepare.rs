use central_repository_dao::user::{Model as UserModel, UpdatableModel};

use crate::{auth::hashing::UserPassword, error::APIError};

pub trait DBPrepare {
    /// Prepare any given object for database insertion.
    fn prepare(&mut self) -> impl std::future::Future<Output = Result<(), APIError>> + Send;
}

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

impl DBPrepare for UpdatableModel {
    /// Conditionally prepare this updatable model for insertion.
    /// This basically checks whether or not the password was updated.
    /// If it was, we just simply generate a hash for it.
    async fn prepare(&mut self) -> Result<(), APIError> {
        let password = match &self.password {
            Some(s) => s.to_owned(),
            _ => return Ok(()),
        };
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
