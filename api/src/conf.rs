use better_debug::BetterDebug;
use central_repository_dao::LimitController;
use envconfig::Envconfig;
use jsonwebtoken::{DecodingKey, EncodingKey};
use lazy_static::lazy_static;
use std::error::Error;

use base64::engine::general_purpose;
use base64::Engine as _;
use log::info;
use once_cell::sync::OnceCell;
use ring::signature::{Ed25519KeyPair, KeyPair};
use sea_orm::DatabaseConnection;

pub static DB_POOL: OnceCell<DatabaseConnection> = OnceCell::new();

lazy_static! {
    // we can safely call unwrap() here because the private key has been already
    // validated (with verify_keys).
    pub static ref CONFIG: Config = Config::init_from_env().unwrap();
    static ref CODING_KEYS: (EncodingKey, DecodingKey) = get_coding_keys(&CONFIG.ed25519_signing_key).unwrap();
    pub static ref ENCODING_KEY: EncodingKey = CODING_KEYS.0.clone();
    pub static ref DECODING_KEY: DecodingKey = CODING_KEYS.1.clone();
    pub static ref BULK_INSERT_CHUNK_SIZE: usize = CONFIG.bulk_insert_chunk_size as usize;
    pub static ref PROTECT_SUPERUSER: bool = CONFIG.protect_superuser;
    pub static ref MAX_PAGINATION_SIZE: u64 = CONFIG.max_pagination_size;
    pub static ref DEFAULT_PAGINATION_SIZE: u64 = CONFIG.default_pagination_size;
    pub static ref RETURN_QUERY_COUNT: bool = CONFIG.return_query_count;
    pub static ref MAX_JSON_PAYLOAD_SIZE: usize = CONFIG.max_json_payload_size as usize;
    pub static ref DB_ACQUIRE_CONNECTION_TIMEOUT_SEC: usize = CONFIG.db_acquire_connection_timeout_sec as usize;
    pub static ref DB_CSV_STREAM_WORKERS: usize = CONFIG.db_csv_stream_workers as usize;
    pub static ref DB_CSV_TRANSFORM_WORKERS: usize = CONFIG.db_csv_transform_workers as usize;
    pub static ref DB_CSV_WORKER_QUEUE_DEPTH: usize = CONFIG.db_csv_worker_queue_depth as usize;
    pub static ref MAX_API_KEYS_PER_USER: usize = CONFIG.max_api_keys_per_user as usize;
    pub static ref TOKEN_API_KEY_EXPIRATION_HOURS: usize = CONFIG.token_api_key_expiration_hours as usize;
    pub static ref LIMIT_SERVICE: LimitController = LimitController::new(CONFIG.db_max_streams_per_user);
}

fn get_coding_keys(key: &String) -> Result<(EncodingKey, DecodingKey), Box<dyn Error>> {
    let decoded = general_purpose::STANDARD.decode(key)?;
    info!("Successfully decoded base64-encoded content, now trying to parse PKCS8 key.");
    let key = Ed25519KeyPair::from_pkcs8_maybe_unchecked(&decoded)?;
    let encoding_key = EncodingKey::from_ed_der(&decoded);
    let decoding_key = DecodingKey::from_ed_der(key.public_key().as_ref());
    info!(
        "Loaded Ed25519 keys, public part is: {:?}. \
Please confirm the public part matches the private key with '\
cat <PRIV_KEY>.pem | openssl pkey -noout -text'",
        key.public_key()
    );
    Ok((encoding_key, decoding_key))
}

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
}

impl Config {
    pub fn verify(&self) -> Result<(), Box<dyn Error>> {
        get_coding_keys(&self.ed25519_signing_key)?;
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
        Ok(())
    }
}
