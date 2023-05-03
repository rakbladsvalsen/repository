use actix_http::header::{HeaderName, HeaderValue};
use lazy_static::lazy_static;
use log::{error, info};
use tracing::{debug_span, field};
use uuid::Uuid;

use crate::common::{create_middleware, handle_fatal};

lazy_static! {
    static ref HEADER_NAME: HeaderName = HeaderName::try_from("Request-Id").unwrap();
    // this should never need to be used.
    static ref INVALID_HEADER_VAL: HeaderValue = HeaderValue::try_from("n/a").unwrap();
}

create_middleware!(
    LogMiddleware,
    LogMiddlewareInner,
    fn call(&self, req: ServiceRequest) -> Self::Future {
        use std::time::Instant;

        let svc = self.service.clone();

        Box::pin(async move {
            let uuid = Uuid::new_v4().to_string();
            let method = req.method().to_string();
            let span = debug_span!("central_repository", id=%uuid, path=%req.path(), query=%req.query_string(), method=%method, user=field::Empty, user_id=field::Empty, superuser=field::Empty).entered();
            // Insert span into request. This span will live until the request
            // extensions get dropped.
            req.extensions_mut().insert(span);
            let start = Instant::now();
            let mut res = svc.call(req).await.map_err(|err| {
                // this should never happen.
                error!(
                    "middleware error: {:?}, status={}",
                    err,
                    err.as_response_error().status_code()
                );
                err
            })?;
            // log end of request.
            let elapsed = start.elapsed();
            let status = res.status();
            // add request id to response.
            res.headers_mut().insert(
                HEADER_NAME.clone(),
                HeaderValue::try_from(uuid).unwrap_or_else(|err| {
                    // this should never happen.
                    handle_fatal!("HeaderValue::try_from", err, INVALID_HEADER_VAL.clone())
                }),
            );
            info!(
                "finished processing request: status: {}, elapsed: {:?}",
                status, elapsed
            );
            Ok(res)
        })
    }
);
