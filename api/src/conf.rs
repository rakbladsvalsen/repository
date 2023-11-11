use central_repository_config::inner::Config;
use central_repository_dao::LimitController;
use jsonwebtoken::{DecodingKey, EncodingKey};
use std::error::Error;

use base64::engine::general_purpose;
use base64::Engine as _;
use log::info;
use once_cell::sync::OnceCell;
use ring::signature::{Ed25519KeyPair, KeyPair};

static ENCODING_KEY: OnceCell<EncodingKey> = OnceCell::new();
static DECODING_KEY: OnceCell<DecodingKey> = OnceCell::new();
static LIMIT_SERVICE: OnceCell<LimitController> = OnceCell::new();

pub struct APIConfig;

impl APIConfig {
    pub fn init_jwt_keys() -> Result<(), Box<dyn Error>> {
        let conf = Config::get();
        let decoded = general_purpose::STANDARD.decode(&conf.ed25519_signing_key)?;
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
        if ENCODING_KEY.set(encoding_key).is_err() {
            return Err("Cannot set encoding key".into());
        }
        if DECODING_KEY.set(decoding_key).is_err() {
            return Err("Cannot set decoding key".into());
        }
        Ok(())
    }

    pub fn init_limit_service() -> Result<(), Box<dyn Error>> {
        let conf = Config::get();
        let service = LimitController::new(conf.db_max_streams_per_user);
        if LIMIT_SERVICE.set(service).is_err() {
            return Err("Cannot set limit service".into());
        }
        Ok(())
    }

    pub fn get_encoding_key() -> &'static EncodingKey {
        ENCODING_KEY.get().expect("encoding key not initialized")
    }

    pub fn get_decoding_key() -> &'static DecodingKey {
        DECODING_KEY.get().expect("decoding key not initialized")
    }

    pub fn get_limit_service() -> &'static LimitController {
        LIMIT_SERVICE.get().expect("limit service not initialized")
    }
}
