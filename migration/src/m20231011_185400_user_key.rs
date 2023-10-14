use entity::{api_key, user};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(APIKey::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(APIKey::Id)
                            .comment("this key's id")
                            .uuid()
                            .unique_key()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(APIKey::UserId)
                            .uuid()
                            .not_null()
                            .comment("Foreign key (owner of this key)"),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            // create foreign key from this api key...
                            .from(api_key::Entity, api_key::Column::UserId)
                            // ..and point it to this user.
                            .to(user::Entity, user::Column::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(APIKey::Active)
                            .comment("Whether this key is active or not")
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(APIKey::CreatedAt)
                            .comment("Created at date")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(APIKey::LastRotatedAt)
                            .comment("Last rotation date")
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("user_api_key_active")
                    .table(APIKey::Table)
                    .col(APIKey::Active)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("user_api_key_user_id")
                    .table(APIKey::Table)
                    .col(APIKey::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(APIKey::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum APIKey {
    Table,
    Id,
    UserId,
    CreatedAt,
    LastRotatedAt,
    Active,
}
