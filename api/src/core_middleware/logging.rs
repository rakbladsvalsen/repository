use chrono::Utc;
use lazy_static::lazy_static;
use log::info;
use tracing::{debug_span, field};
use uuid::Uuid;

use crate::common::{create_middleware, format_duration};

lazy_static! {
    static ref BEARER: &'static str = "Bearer ";
}

create_middleware!(
    LogMiddleware,
    LogMiddlewareInner,
    fn call(&self, req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let uuid = Uuid::new_v4().to_string();
            let method = req.method().to_string();
            let span = debug_span!("central_repository", id=%uuid, path=%req.path(), query=%req.query_string(), user=field::Empty, method=%method).entered();
            // Insert span into request. This span will live until the request
            // extensions get dropped.
            req.extensions_mut().insert(span);
            let start = Utc::now().time();
            let res = svc.call(req).await?;
            // log end of request.
            let elapsed = format_duration(start);
            let status = res.status();
            info!("finished: status={}, elapsed={}", status, elapsed);
            Ok(res)
        })
    }
);
