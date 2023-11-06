use entity::{format, format_entitlement, user};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(FormatEntitlement::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(FormatEntitlement::UserId).uuid().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            // create foreign key from this record...
                            .from(
                                format_entitlement::Entity,
                                format_entitlement::Column::UserId,
                            )
                            // ..and point it to user
                            .to(user::Entity, user::Column::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(FormatEntitlement::FormatId)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            // create foreign key from this record...
                            .from(
                                format_entitlement::Entity,
                                format_entitlement::Column::FormatId,
                            )
                            // ..and point it to format
                            .to(format::Entity, format::Column::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(FormatEntitlement::Access)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FormatEntitlement::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        sea_query::Index::create()
                            .col(FormatEntitlement::FormatId)
                            .col(FormatEntitlement::UserId),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FormatEntitlement::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum FormatEntitlement {
    Table,
    CreatedAt,
    UserId,
    FormatId,
    Access,
}
