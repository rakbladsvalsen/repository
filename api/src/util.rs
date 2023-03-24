use actix_example_core::user::Model as UserModel;
use actix_web::web::ReqData;
use log::info;

use crate::error::APIError;

pub fn verify_admin(user: &ReqData<UserModel>) -> Result<(), APIError> {
    if !user.is_superuser {
        // only admins can create new users.
        info!("Denied access to admin resource, user id: {}", user.id);
        return Err(APIError::AdminOnlyResource);
    }
    Ok(())
}
