use better_debug::BetterDebug;
use dotenvy::dotenv;
use envconfig::Envconfig;
use log::{info, warn};
use once_cell::sync::OnceCell;
use std::error::Error;

pub static CONFIG: OnceCell<Config> = OnceCell::new();

#[derive(Envconfig, BetterDebug)]
pub struct Config {
    #[better_debug(secret)]
    #[envconfig(from = "DATABASE_URL")]
    pub database_url: String,

    #[envconfig(from = "HTTP_ADDRESS", default = "127.0.0.1")]
    pub http_address: String,

    #[envconfig(from = "HTTP_PORT", default = "8000")]
    pub http_port: u16,

    #[envconfig(from = "DB_POOL_MAX_CONN", default = "100")]
    pub db_pool_max_conn: u32,

    #[envconfig(from = "DB_POOL_MIN_CONN", default = "10")]
    pub db_pool_min_conn: u32,

    #[better_debug(secret)]
    #[envconfig(from = "ED25519_SIGNING_KEY")]
    pub ed25519_signing_key: String,

    #[envconfig(from = "TOKEN_EXPIRATION_SECONDS", default = "300")]
    pub token_expiration_seconds: u32,

    #[envconfig(from = "BULK_INSERT_CHUNK_SIZE", default = "200")]
    pub bulk_insert_chunk_size: u32,

    #[envconfig(from = "PROTECT_SUPERUSER", default = "true")]
    pub protect_superuser: bool,

    #[envconfig(from = "MAX_PAGINATION_SIZE", default = "1000")]
    pub max_pagination_size: u64,

    #[envconfig(from = "DEFAULT_PAGINATION_SIZE", default = "1000")]
    pub default_pagination_size: u64,

    #[envconfig(from = "WORKERS", default = "16")]
    pub workers: u8,

    #[envconfig(from = "RETURN_QUERY_COUNT", default = "true")]
    pub return_query_count: bool,

    // set by default to 100_000 bytes (100 KB)
    #[envconfig(from = "MAX_JSON_PAYLOAD_SIZE", default = "100000")]
    pub max_json_payload_size: u64,

    #[envconfig(from = "DB_ACQUIRE_CONNECTION_TIMEOUT_SEC", default = "30")]
    pub db_acquire_connection_timeout_sec: u64,

    #[envconfig(from = "DB_CSV_STREAM_WORKERS", default = "1")]
    pub db_csv_stream_workers: u64,

    #[envconfig(from = "DB_CSV_TRANSFORM_WORKERS", default = "2")]
    pub db_csv_transform_workers: u64,

    #[envconfig(from = "DB_CSV_WORKER_QUEUE_DEPTH", default = "200")]
    pub db_csv_worker_queue_depth: u64,

    #[envconfig(from = "MAX_API_KEYS_PER_USER", default = "10")]
    pub max_api_keys_per_user: u64,

    // set by default to 1 month (24 * 30 = 720)
    #[envconfig(from = "TOKEN_API_KEY_EXPIRATION_HOURS", default = "720")]
    pub token_api_key_expiration_hours: u64,

    #[envconfig(from = "DB_MAX_STREAMS_PER_USER", default = "2")]
    pub db_max_streams_per_user: u64,

    // For users with temporal delete permission, allow them
    // to delete entries uploaded in the last N hours.
    #[envconfig(from = "TEMPORAL_DELETE_HOURS", default = "24")]
    pub temporal_delete_hours: u64,

    // Whether or not to enable the prune old data job. This will
    // spawn a background thread to delete old data on a per-format
    // basis.
    // Default: enabled
    #[envconfig(from = "ENABLE_PRUNE_JOB", default = "true")]
    pub enable_prune_job: bool,

    // If ENABLE_PRUNE_JOB is enabled, then the task will be executed
    // every PRUNE_JOB_RUN_INTERVAL_SECONDS.
    // Default: 600 seconds (10 minutes)
    #[envconfig(from = "PRUNE_JOB_RUN_INTERVAL_SECONDS", default = "600")]
    pub prune_job_run_interval_seconds: u64,

    // Kill the prune job timeout after PRUNE_JOB_TIMEOUT seconds.
    // Default: 300 seconds (5 minutes).
    #[envconfig(from = "PRUNE_JOB_TIMEOUT_SECONDS", default = "300")]
    pub prune_job_timeout_seconds: u64,
}

impl Config {
    pub fn init_and_check() -> Result<&'static Config, Box<dyn Error>> {
        if CONFIG.get().is_some() {
            warn!("init_and_check() was called twice!");
            return Ok(CONFIG.get().expect("config: Cannot get inner struct"));
        }
        dotenv().ok();
        info!("reading config from environment");
        let config = Config::init_from_env()?;
        config.verify()?;
        info!("config: OK: {:#?}", config);
        CONFIG.set(config).expect("config: Cannot set inner struct");
        Ok(CONFIG.get().expect("config: Cannot get inner struct"))
    }

    pub fn get() -> &'static Config {
        CONFIG.get().expect("config: NOT INITIALIZED!")
    }

    pub fn verify(&self) -> Result<(), Box<dyn Error>> {
        if self.max_pagination_size == 0 {
            return Err("MAX_PAGINATION_SIZE must be greater than 0".into());
        }
        if self.default_pagination_size == 0 {
            return Err("DEFAULT_PAGINATION_SIZE must be greater than 0".into());
        }
        if self.default_pagination_size > self.max_pagination_size {
            return Err("DEFAULT_PAGINATION_SIZE must be less than MAX_PAGINATION_SIZE".into());
        }
        if self.bulk_insert_chunk_size == 0 {
            return Err("BULK_INSERT_CHUNK_SIZE must be greater than 0".into());
        }
        if self.token_expiration_seconds == 0 {
            return Err("TOKEN_EXPIRATION_SECONDS must be greater than 0".into());
        }
        if self.db_pool_min_conn == 0 {
            return Err("DB_POOL_MIN_CONN must be greater than 0".into());
        }
        if self.db_pool_min_conn > self.db_pool_max_conn {
            return Err("DB_POOL_MIN_CONN must be less than DB_POOL_MAX_CONN".into());
        }
        if self.workers == 0 {
            return Err("WORKERS must be greater than 0".into());
        }
        if self.max_json_payload_size == 0 {
            return Err("MAX_JSON_PAYLOAD_SIZE must be greater than 0".into());
        }
        if self.db_acquire_connection_timeout_sec == 0 {
            return Err("DB_ACQUIRE_CONNECTION_TIMEOUT_SEC must be greater than 0".into());
        }
        if self.db_csv_stream_workers == 0 {
            return Err("DB_CSV_STREAM_WORKERS must be greater than 0".into());
        }
        if self.db_csv_transform_workers == 0 {
            return Err("DB_CSV_TRANSFORM_WORKERS must be greater than 0".into());
        }
        if self.db_csv_worker_queue_depth == 0 {
            return Err("DB_CSV_WORKER_QUEUE_DEPTH must be greater than 0".into());
        }
        if self.max_api_keys_per_user == 0 {
            return Err("MAX_API_KEYS_PER_USER must be greater than 0".into());
        }
        if self.token_api_key_expiration_hours == 0 {
            return Err("TOKEN_API_KEY_EXPIRATION_HOURS must be greater than 0".into());
        }
        if self.db_max_streams_per_user == 0 {
            return Err("DB_MAX_STREAMS_PER_USER must be greater than 0".into());
        }
        if self.temporal_delete_hours == 0 {
            return Err("TEMPORAL_DELETE_HOURS must be greater than 0".into());
        }
        if self.enable_prune_job {
            if self.prune_job_run_interval_seconds == 0 {
                return Err("PRUNE_JOB_RUN_INTERVAL_SECONDS must be greater than 0".into());
            }
            if self.prune_job_timeout_seconds == 0 {
                return Err("PRUNE_JOB_TIMEOUT_SECONDS must be greater than 0".into());
            }
        }

        Ok(())
    }
}
