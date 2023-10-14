use crate::common::handle_fatal;
use crate::error::APIError;
use argon2::password_hash::Error as ArgonError;
use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use log::error;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use super::jwt::ARGON;

pub struct UserPassword {
    password: String,
}

impl From<String> for UserPassword {
    /// Convert a plaintext password to a 'password' instance
    fn from(value: String) -> Self {
        UserPassword { password: value }
    }
}

pub trait StringHashUtil {
    /// Validate any string against the given "legit password:.
    /// `true_password` must contain a valid hash, else an error will
    /// be returned.
    fn try_validate_against_hash(&self, true_password: impl AsRef<str>) -> Result<(), APIError>;

    /// Converts the string to an argon hash using the recommended defaults.
    fn try_get_argon_hash(&self) -> Result<String, APIError>;

    /// Generates a random, secure string of len `size`.
    fn new_random(size: usize) -> String;
}

impl StringHashUtil for String {
    #[inline(always)]
    fn try_get_argon_hash(&self) -> Result<String, APIError> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(ARGON
            .hash_password((self).as_bytes(), &salt)
            .map_err(|err| handle_fatal!("password hashing error", err, APIError::ServerError))?
            .to_string())
    }

    #[inline(always)]
    fn try_validate_against_hash(&self, legit_password: impl AsRef<str>) -> Result<(), APIError> {
        let parsed_hash = PasswordHash::new(legit_password.as_ref()).map_err(|err| {
            handle_fatal!("cannot unwrap legitimate hash", err, APIError::ServerError)
        })?;
        let check = ARGON.verify_password(self.as_bytes(), &parsed_hash);
        match check {
            // invalid password
            Ok(_) => Ok(()),
            Err(ArgonError::Password) => Err(APIError::InvalidCredentials),
            // this should never happen
            Err(unhandled) => {
                handle_fatal!(
                    "argon2 verification error",
                    unhandled,
                    Err(APIError::ServerError)
                )
            }
        }
    }

    #[inline(always)]
    fn new_random(sample: usize) -> String {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .map(char::from) // map added here
            .take(sample)
            .collect()
    }
}

impl UserPassword {
    pub fn to_hash(&self) -> Result<String, APIError> {
        self.password.try_get_argon_hash()
    }

    /// verify whether a password matches a known stored hash.
    pub fn verify_password(user_input: &String, true_password: &str) -> Result<(), APIError> {
        user_input.try_validate_against_hash(true_password)
    }
}
