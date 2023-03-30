use crate::traits::{AsQueryParamFilterable, AsQueryParamSortable};
use central_repository_macros::AsQueryParam;
use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use sea_orm::Value;
use serde::{Deserialize, Serialize};

#[derive(EnumIter, DeriveActiveEnum, Eq, PartialEq, Deserialize, Serialize, Debug, Clone)]
#[sea_orm(rs_type = "String", db_type = "String(None)")]
#[serde(rename_all = "camelCase")]
pub enum Access {
    #[sea_orm(string_value = "RW")]
    ReadWrite,
    #[sea_orm(string_value = "R")]
    ReadOnly,
    #[sea_orm(string_value = "W")]
    WriteOnly,
}

impl From<&Access> for Value {
    // We need to implement From<&Access> as we otherwise
    // won't be able to use as_query(eq) for any field with
    // type Access.
    fn from(value: &Access) -> Self {
        sea_orm::Value::from(value.to_value())
    }
}

impl Default for Access {
    fn default() -> Self {
        Self::ReadOnly
    }
}

#[derive(
    AsQueryParam, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize, Default,
)]
#[serde(rename_all = "camelCase")]
#[as_query(sort_default_column = "Column::CreatedAt")]
#[sea_orm(table_name = "format_entitlement")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
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
        column = "Column::FormatId",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "*value"
    )]
    #[sea_orm(primary_key, auto_increment = false)]
    pub format_id: i32,
    #[serde(skip_deserializing, default = "chrono::offset::Utc::now")]
    #[as_query(
        column = "Column::CreatedAt",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "sea_orm::Value::from(*value)"
    )]
    pub created_at: DateTime<Utc>,
    #[as_query(column = "Column::Access", eq)]
    pub access: Access,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchModel {
    pub user_id: i32,
    pub format_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // define inverse relation
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::format::Entity",
        from = "Column::FormatId",
        to = "super::format::Column::Id"
    )]
    Format,
}
impl ActiveModelBehavior for ActiveModel {}
