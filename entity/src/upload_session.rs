use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(EnumIter, DeriveActiveEnum, Eq, PartialEq, Deserialize, Serialize, Debug, Clone)]
#[sea_orm(rs_type = "String", db_type = "String(None)")]
pub enum OutcomeKind {
    #[sea_orm(string_value = "SUCCESS")]
    Success,
    #[sea_orm(string_value = "ERROR")]
    Error,
}

impl Default for OutcomeKind {
    fn default() -> Self {
        Self::Error
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "upload_session")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[serde(skip_deserializing)]
    pub id: i32,
    // This one won't be user-accessible too
    #[serde(skip_deserializing, default = "chrono::offset::Utc::now")]
    pub created_at: DateTime<Utc>,
    pub record_count: i32,
    // foreign key to format id
    pub format_id: i32,
    // foreign key to user id
    pub user_id: i32,
    pub outcome: OutcomeKind,
    pub detail: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // define one-to-many relation
    #[sea_orm(has_many = "super::record::Entity")]
    Record,
}

impl Related<super::record::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Record.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
