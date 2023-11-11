use std::time::Duration;

use central_repository_config::inner::Config;
use log::{info, warn};
use once_cell::sync::OnceCell;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};

pub static CONNECTION: OnceCell<DatabaseConnection> = OnceCell::new();

pub struct DBConfig;

impl DBConfig {
    pub async fn init_db_connection() -> Result<(), Box<dyn std::error::Error>> {
        if CONNECTION.get().is_some() {
            warn!("init_db_connection() called twice!");
            return Ok(());
        }
        let config = Config::get();
        let mut opt = ConnectOptions::new(&config.database_url);
        // configure thread pool
        opt.max_connections(config.db_pool_max_conn)
            .min_connections(config.db_pool_min_conn)
            .acquire_timeout(Duration::from_secs(
                config.db_acquire_connection_timeout_sec,
            ));

        info!("Trying to create database pool...");
        let conn = Database::connect(opt).await?;
        CONNECTION
            .set(conn)
            .expect("Cannot set database connection");
        Ok(())
    }

    /// Get a reference to the database connection.
    pub fn get_connection() -> &'static DatabaseConnection {
        CONNECTION
            .get()
            .expect("Database connection not initialized")
    }
}
