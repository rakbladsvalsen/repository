use actix_http::header;
use lazy_static::lazy_static;
use log::{debug, info};
use tracing::span::EnteredSpan;

use crate::{auth::jwt::Token, common::create_middleware, error::APIError};

lazy_static! {
    static ref BEARER: &'static str = "Bearer ";
}

// This middleware authenticates any incoming request
// containing a Bearer: Token.
// The token can be of any type: it gets passed through to the
// validation function.
create_middleware!(
    AuthMiddleware,
    AuthMiddlewareInner,
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        let token = req
            .headers()
            .get(header::AUTHORIZATION)
            .map(|h| h.to_str().unwrap_or("").to_string());

        if token.is_none() {
            info!("No auth token found, returning 401");
            return Box::pin(
                async move { Ok(req.error_response(APIError::MissingAuthHeader).into()) },
            );
        }

        Box::pin(async move {
            let token = token.unwrap_or("".to_string());
            // Basic HTTP authentication
            if token.len() < BEARER.len() || !token.starts_with(&BEARER as &str) {
                info!(
                    "Auth error: token is either empty or doesn't start with '{}'",
                    *BEARER
                );
                return Ok(req.error_response(APIError::MissingAuthHeader).into());
            }
            // Trim "Bearer " part; we don't need it.
            let token = token[BEARER.len()..].to_string();
            debug!("token length: {}", token.len());

            let token = Token::from(token);

            // handle token validation
            let user = match token.validate().await {
                Err(err) => return Ok(req.error_response(err).into()),
                Ok(user) => user,
            };

            // add authenticated user to logging span
            // note: we need to drop `extensions` to use `req` again
            {
                let extensions = req.extensions();
                let span = extensions
                    .get::<EnteredSpan>()
                    .expect("Logging middleware must run before auth middleware!");
                span.record("user", &user.username);
                span.record("user_id", user.id.to_string());
                span.record("superuser", user.is_superuser);
            }
            info!(
                "Authenticated token for user id: {}, username: {:?}.",
                user.id, user.username
            );
            req.extensions_mut().insert(user);
            svc.call(req).await
        })
    }
);
