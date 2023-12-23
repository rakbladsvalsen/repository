pub mod api_key;
pub mod auth;
pub mod common;
pub mod conf;
pub mod core_middleware;
pub mod error;
pub mod format;
pub mod format_entitlement;
pub mod model_prepare;
pub mod pagination;
pub mod record;
pub mod record_validation;
pub mod upload_session;
pub mod user;
pub mod util;

use std::error::Error;

use actix_web::{App, HttpServer};
use central_repository_config::{self, inner::Config};
use central_repository_dao::{conf::DBConfig, tasks::Tasks};
use format::init_format_routes;
use format_entitlement::init_format_entitlement_routes;
use log::info;
use migration::{Migrator, MigratorTrait};
use mimalloc::MiMalloc;
use record::init_record_routes;
use user::init_user_routes;

use crate::{
    conf::APIConfig,
    core_middleware::logging::LogMiddleware,
    error::{json_error_handler, path_error_handler, query_error_handler},
    upload_session::init_upload_session_routes,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[actix_web::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let config = Config::init_and_check()?;
    APIConfig::init_jwt_keys()?;
    APIConfig::init_limit_service()?;
    DBConfig::init_db_connection().await?;

    // run pending migrations
    Migrator::up(DBConfig::get_connection(), None).await?;

    Tasks::init_prune_task();

    info!(
        "Launching server on {}:{}",
        config.http_address, config.http_port
    );
    HttpServer::new(move || {
        App::new()
            .wrap(LogMiddleware)
            .app_data(json_error_handler())
            .app_data(query_error_handler())
            .app_data(path_error_handler())
            .configure(init_format_routes)
            .configure(init_record_routes)
            .configure(init_user_routes)
            .configure(init_format_entitlement_routes)
            .configure(init_upload_session_routes)
    })
    .bind(format!("{}:{}", config.http_address, config.http_port))?
    .workers(config.workers.into())
    .run()
    .await?;
    Ok(())
}
