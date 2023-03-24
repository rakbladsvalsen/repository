use std::{cell::RefCell, rc::Rc};

use actix_example_core::sea_orm::DatabaseConnection;
use chrono::{NaiveTime, Utc};

pub type RcRefCell<T> = Rc<RefCell<T>>;

#[derive(Clone, Debug)]
pub struct AppState {
    pub conn: DatabaseConnection,
}

#[inline(always)]
pub fn format_duration(start: NaiveTime) -> String {
    let diff = Utc::now().time() - start;
    match diff.num_microseconds() {
        Some(duration) => {
            if duration > 1_000_000 {
                format!("{} s", duration as f64 / 100000.0)
            } else if duration > 1_000 {
                format!("{} ms", duration as f64 / 1000.0)
            } else {
                format!("{} μs", duration)
            }
        }
        None => format!("{} ms", diff.num_milliseconds()),
    }
}

/// Returns the time taken for a function (be it sync or async) to complete
macro_rules! timed {
    ($description:expr, $function:expr) => {{
        // imports
        use log::debug;
        use sqlx::types::chrono::Utc;

        // the real good stuff
        let start = Utc::now().time();
        let result = $function;
        let diff = Utc::now().time() - start;
        let duration = match diff.num_microseconds() {
            Some(duration) => {
                if duration > 1_000_000 {
                    format!("{} s", duration as f64 / 100000.0)
                } else if duration > 1_000 {
                    format!("{} ms", duration as f64 / 1000.0)
                } else {
                    format!("{} μs", duration)
                }
            }
            None => format!("{} ms", diff.num_milliseconds()),
        };
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
