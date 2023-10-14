pub use sea_orm_migration::prelude::*;

mod m20230220_183928_create_user;
mod m20230220_192731_format;
mod m20230221_184209_session;
mod m20230221_195143_record;
mod m20230315_035330_create_format_entitlement;
mod m20231011_185400_user_key;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20230220_183928_create_user::Migration),
            Box::new(m20230220_192731_format::Migration),
            Box::new(m20230221_184209_session::Migration),
            Box::new(m20230221_195143_record::Migration),
            Box::new(m20230315_035330_create_format_entitlement::Migration),
            Box::new(m20231011_185400_user_key::Migration),
        ]
    }
}
