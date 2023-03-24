use std::error::Error;

use envconfig::Envconfig;
use jsonwebtoken::{DecodingKey, EncodingKey};
use lazy_static::lazy_static;

use base64::engine::general_purpose;
use base64::Engine as _;
use log::info;
use ring::signature::{Ed25519KeyPair, KeyPair};

lazy_static! {
    // we can safely call unwrap() here because the private key has been already
    // validated (with verify_keys).
    pub static ref CONFIG: Config = Config::init_from_env().unwrap();
    static ref CODING_KEYS: (EncodingKey, DecodingKey) = get_coding_keys(&CONFIG.ed25519_signing_key).unwrap();
    pub static ref ENCODING_KEY: EncodingKey = CODING_KEYS.0.clone();
    pub static ref DECODING_KEY: DecodingKey = CODING_KEYS.1.clone();
    pub static ref BULK_INSERT_CHUNK_SIZE: usize = CONFIG.bulk_insert_chunk_size as usize;
    pub static ref PROTECT_SUPERUSER: bool = CONFIG.protect_superuser;
}

fn get_coding_keys(key: &String) -> Result<(EncodingKey, DecodingKey), Box<dyn Error>> {
    let decoded = general_purpose::STANDARD.decode(key)?;
    info!("Successfully decoded base64-encoded content, now trying to parse PKCS8 key.");
    let key = Ed25519KeyPair::from_pkcs8_maybe_unchecked(&decoded)?;
    let encoding_key = EncodingKey::from_ed_der(&decoded);
    let decoding_key = DecodingKey::from_ed_der(key.public_key().as_ref());
    info!("Loaded Ed25519 key.");
    Ok((encoding_key, decoding_key))
}

#[derive(Envconfig, Debug)]
pub struct Config {
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

    #[envconfig(from = "ED25519_SIGNING_KEY")]
    pub ed25519_signing_key: String,

    #[envconfig(from = "TOKEN_EXPIRATION_SECONDS", default = "300")]
    pub token_expiration_seconds: u32,

    #[envconfig(from = "BULK_INSERT_CHUNK_SIZE", default = "200")]
    pub bulk_insert_chunk_size: u32,

    #[envconfig(from = "PROTECT_SUPERUSER", default = "true")]
    pub protect_superuser: bool,
}

impl Config {
    pub fn verify_keys(&self) -> Result<(), Box<dyn Error>> {
        get_coding_keys(&self.ed25519_signing_key).map(|_| Ok(()))?
    }
}
