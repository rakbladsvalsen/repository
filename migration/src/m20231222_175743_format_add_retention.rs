/// This migration only adds a new column to the Format table. It
/// doesn't actually create a new table, like all the other migrations.
use sea_orm_migration::prelude::*;

use crate::m20230220_192731_format::Format;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Format::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Format::RetentionPeriodMinutes)
                            .unsigned()
                            .not_null()
                            // 525600 = 24 * 60 * 365 (~1 year)
                            .default(525600),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Format::Table)
                    .drop_column(Format::RetentionPeriodMinutes)
                    .to_owned(),
            )
            .await
    }
}
