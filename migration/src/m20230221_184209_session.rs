use entity::{format, upload_session, user};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UploadSession::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UploadSession::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(UploadSession::RecordCount)
                            .unsigned()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UploadSession::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(UploadSession::UserId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            // create foreign key from this record...
                            .from(upload_session::Entity, upload_session::Column::UserId)
                            // ..and point it to user
                            .to(user::Entity, format::Column::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(UploadSession::FormatId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            // create foreign key from this record...
                            .from(upload_session::Entity, upload_session::Column::FormatId)
                            // ..and point it to user
                            .to(format::Entity, format::Column::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(UploadSession::Outcome).string().not_null())
                    .col(ColumnDef::new(UploadSession::Detail).string().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UploadSession::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum UploadSession {
    Table,
    Id,
    CreatedAt,
    FormatId,
    UserId,
    RecordCount,
    Outcome,
    Detail,
}
