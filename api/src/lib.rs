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

use std::{error::Error, time::Duration};

use actix_web::{App, HttpServer};
use central_repository_dao::sea_orm::{ConnectOptions, Database};
use conf::Config;
use envconfig::Envconfig;
use format::init_format_routes;
use format_entitlement::init_format_entitlement_routes;
use log::info;
use migration::{Migrator, MigratorTrait};
use mimalloc::MiMalloc;
use record::init_record_routes;
use user::init_user_routes;

use crate::{
    conf::DB_POOL,
    core_middleware::logging::LogMiddleware,
    error::{json_error_handler, path_error_handler, query_error_handler},
    upload_session::init_upload_session_routes,
};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[actix_web::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    // get env vars
    dotenvy::dotenv().ok();
    let config = Config::init_from_env()?;
    // make sure config is valid
    config.verify()?;

    info!("Using config: {:#?}", config);
    let mut opt = ConnectOptions::new(config.database_url);
    // configure thread pool
    opt.max_connections(config.db_pool_max_conn)
        .min_connections(config.db_pool_min_conn)
        .acquire_timeout(Duration::from_secs(
            config.db_acquire_connection_timeout_sec,
        ));

    info!("Trying to create database pool...");
    let conn = Database::connect(opt).await.map(|r| {
        info!("Database pool successfully created.");
        r
    })?;
    // run pending migrations
    Migrator::up(&conn, None).await?;
    // initialize OnceCell with database
    DB_POOL.set(conn).unwrap();

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
