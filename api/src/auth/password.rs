use crate::common::handle_fatal;
use crate::error::APIError;
use argon2::password_hash::Error as ArgonError;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use lazy_static::lazy_static;
use log::{error, info};

lazy_static! {
    static ref ARGON: Argon2<'static> = Argon2::default();
}

pub struct UserPassword {
    password: String,
}

impl From<String> for UserPassword {
    /// Convert a plaintext password to a 'password' instance
    fn from(value: String) -> Self {
        UserPassword { password: value }
    }
}

impl TryFrom<UserPassword> for String {
    type Error = APIError;

    /// Converts this password to a hashed one (if possible).
    fn try_from(value: UserPassword) -> Result<Self, Self::Error> {
        value.perform()
    }
}

impl UserPassword {
    fn perform(&self) -> Result<String, APIError> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(ARGON
            .hash_password(self.password.as_bytes(), &salt)
            .map_err(|err| handle_fatal!("password hashing error", err, APIError::ServerError))?
            .to_string())
    }

    /// verify whether a password matches a known stored hash.
    pub fn verify_password(user_input: &String, true_password: &str) -> Result<(), APIError> {
        let parsed_hash = PasswordHash::new(true_password)
            .map_err(|err| handle_fatal!("stored hash parsing", err, APIError::ServerError))?;
        let check = ARGON.verify_password(user_input.as_bytes(), &parsed_hash);
        match check {
            // invalid password
            Ok(_) => Ok(info!("successfully validated password")),
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
}
