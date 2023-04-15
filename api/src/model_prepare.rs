use async_trait::async_trait;
use central_repository_dao::user::Model as UserModel;

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
        let current_span = tracing::Span::current();
        self.password = actix_web::web::block(move || {
            let _guard = current_span.enter();
            UserPassword::from(password).try_into()
        })
        .await??;
        Ok(())
    }
}
