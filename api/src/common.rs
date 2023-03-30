use std::{cell::RefCell, rc::Rc};

use central_repository_dao::sea_orm::DatabaseConnection;

pub type RcRefCell<T> = Rc<RefCell<T>>;

#[derive(Clone, Debug)]
pub struct AppState {
    pub conn: DatabaseConnection,
}

/// Returns the time taken for a function (be it sync or async) to complete
macro_rules! timed {
    ($description:expr, $function:expr) => {{
        use log::debug;
        use std::time::Instant;

        let start = Instant::now();
        let result = $function;
        let elapsed = start.elapsed();
        let duration = format!("{:?}", elapsed);
        debug!(
            "operation '{}' finished, elapsed: {}",
            $description, duration
        );
        result
    }};
}

pub(crate) use timed;

/// Creates a generic middleware.
macro_rules! create_middleware {
    ($name:ident, $innername:ident, $func:item) => {
        use std::{
            cell::RefCell,
            future::{ready, Ready},
            pin::Pin,
            rc::Rc,
        };

        use actix_http::HttpMessage;
        use actix_web::{
            dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform}, Error,
        };
        type RcRefCell<T> = Rc<RefCell<T>>;

        use futures::Future;



        pub struct $name;

        impl<S, B> Transform<S, ServiceRequest> for $name
        where
            S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
            S::Future: 'static,
            B: 'static,
        {
            type Response = ServiceResponse<B>;
            type Error = Error;
            type InitError = ();
            type Transform = $innername<S>;
            type Future = Ready<Result<Self::Transform, Self::InitError>>;

            fn new_transform(&self, service: S) -> Self::Future {
                ready(Ok($innername {
                    service: Rc::new(RefCell::new(service)),
                }))
            }
        }

        pub struct $innername<S> {
            service: RcRefCell<S>,
        }

        impl<S, B> Service<ServiceRequest> for $innername <S>
        where
            S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
            S::Future: 'static,
            B: 'static,
        {
            type Response = ServiceResponse<B>;
            type Error = Error;
            type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

            forward_ready!(service);

            // execute macro func
            $func
        }

    };
}

pub(crate) use create_middleware;
