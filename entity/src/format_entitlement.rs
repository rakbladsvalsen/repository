use std::collections::HashSet;
use std::ops::Deref;

use crate::traits::{AsQueryParamFilterable, AsQueryParamSortable};
use central_repository_macros::AsQueryParam;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::BinOper;
use sea_orm::{entity::prelude::*, FromJsonQueryResult};
use serde::{Deserialize, Serialize};

pub const ARRAY_CONTAINS_OP: BinOper = BinOper::Custom("?");

#[derive(Eq, PartialEq, Deserialize, Serialize, Debug, Clone, FromJsonQueryResult, Hash)]
#[serde(rename_all = "camelCase")]
pub enum AccessLevel {
    Read,
    Write,
    /// Users with this access level will be able to
    /// delete records from the last N hours.
    LimitedDelete,
    /// Users with this access level will be able to
    /// delete whatever records they want.
    Delete,
}

impl AccessLevel {
    #[inline(always)]
    pub fn get_serialized(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("unexpected error: access level encode")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, FromJsonQueryResult, Default)]
pub struct Access(pub HashSet<AccessLevel>);

impl Deref for Access {
    type Target = HashSet<AccessLevel>;
    fn deref(&self) -> &Self::Target {
        &self.0
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
    pub user_id: Uuid,
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
    pub access: Access,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchModel {
    pub user_id: Uuid,
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
