use actix_http::header;
use actix_web::web;
use lazy_static::lazy_static;
use log::{debug, info};
use tracing::span::EnteredSpan;

use crate::{
    auth::jwt::Token,
    common::{create_middleware, AppState},
    error::APIError,
};

lazy_static! {
    static ref BEARER: &'static str = "Bearer ";
}

create_middleware!(
    AuthMiddleware,
    AuthMiddlewareInner,
    fn call(&self, req: ServiceRequest) -> Self::Future {
        use central_repository_dao::user::Model as UserModel;
        let svc = self.service.clone();
        let app_state = req.app_data::<web::Data<AppState>>().unwrap();
        // just clone the db reference, not the whole app state
        let db = app_state.conn.clone();
        let token = req
            .headers()
            .get(header::AUTHORIZATION)
            .map(|h| h.to_str().unwrap_or("").to_string());

        if token.is_none() {
            info!("Missing token for request: {:?}", &req);
            return Box::pin(async move { Err(APIError::MissingAuthHeader.into()) });
        }

        Box::pin(async move {
            let token = token.unwrap_or("".to_string());
            // Basic HTTP authentication
            if token.len() < BEARER.len() || !token.starts_with(&BEARER as &str) {
                info!(
                    "Auth error: token is either empty or doesn't start with '{}'",
                    *BEARER
                );
                info!("Token was: '{}'", token);
                return Err(APIError::MissingAuthHeader.into());
            }
            // Trim "Bearer " part; we don't need it.
            let token = token[BEARER.len()..].to_string();
            debug!("token length: {}", token.len());

            let token = Token::from(token);
            let user: UserModel = token.validate(&db).await?;
            // add authenticated user to logging span
            // note: we need to drop `extensions` to use `req` again
            {
                let extensions = req.extensions();
                let span = extensions
                    .get::<EnteredSpan>()
                    .expect("Logging middleware must run before auth middleware!");
                span.record("user", format!("{}/{}", user.id, user.username));
                span.record("superuser", user.is_superuser);
            }
            info!(
                "Authenticated token for user id: {:?}, username: {:?}.",
                user.id, user.username
            );
            req.extensions_mut().insert(user);
            svc.call(req).await
        })
    }
);
