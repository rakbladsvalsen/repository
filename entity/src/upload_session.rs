use crate::traits::{AsQueryParamFilterable, AsQueryParamSortable};
use central_repository_macros::AsQueryParam;
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

#[derive(
    AsQueryParam, Default, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize,
)]
#[as_query(sort_default_column = "Column::Id", camel_case)]
#[sea_orm(table_name = "upload_session")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[serde(skip_deserializing)]
    #[as_query(column = "Column::Id", eq, lt, gt, lte, gte, custom_convert = "*value")]
    pub id: i32,
    // This one won't be user-accessible too
    #[serde(skip_deserializing, default = "chrono::offset::Utc::now")]
    #[as_query(
        eq,
        lt,
        gt,
        lte,
        gte,
        column = "Column::CreatedAt",
        custom_convert = "*value"
    )]
    pub created_at: DateTime<Utc>,
    #[as_query(
        column = "Column::RecordCount",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "*value"
    )]
    pub record_count: i32,
    // foreign key to format id
    #[as_query(
        column = "Column::FormatId",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "*value"
    )]
    pub format_id: i32,
    // foreign key to user id
    #[as_query(
        column = "Column::UserId",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "*value"
    )]
    pub user_id: i32,
    #[as_query(
        column = "Column::Outcome",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "sea_orm::Value::from(value.to_value())"
    )]
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
