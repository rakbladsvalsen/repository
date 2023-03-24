use entity::{record, upload_session};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Replace the sample below with your own migration scripts

        manager
            .create_table(
                Table::create()
                    .table(Record::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Record::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Record::Data).json_binary().not_null())
                    .col(ColumnDef::new(Record::UploadSessionId).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            // create foreign key from this record...
                            .from(record::Entity, record::Column::UploadSessionId)
                            // ..and point it to upload_session
                            .to(upload_session::Entity, upload_session::Column::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        // create index on foreign key to speed up operations
        manager
            .create_index(
                Index::create()
                    .name("upload_session_fk")
                    .table(Record::Table)
                    .col(Record::UploadSessionId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Record::Table).to_owned())
            .await
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Record {
    Table,
    Id,
    UploadSessionId,
    Data,
}
