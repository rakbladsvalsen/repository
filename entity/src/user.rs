use crate::traits::AsQueryParamFilterable;
use crate::traits::AsQueryParamSortable;
use better_debug::BetterDebug;
use central_repository_macros::AsQueryParam;
use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::extension::postgres::PgBinOper;
use sea_orm::sea_query::Expr;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Default,
    Clone,
    BetterDebug,
    PartialEq,
    Eq,
    DeriveEntityModel,
    Deserialize,
    Serialize,
    AsQueryParam,
)]
#[as_query(sort_default_column = "Column::CreatedAt", camel_case)]
#[sea_orm(table_name = "user")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    // Don't let users define this field.
    #[as_query(column = "Column::Id", eq, lt, gt, lte, gte, custom_convert = "*value")]
    #[serde(skip_deserializing)]
    #[sea_orm(primary_key)]
    pub id: Uuid,
    #[as_query(column = "Column::Username", eq, like, ilike, contains)]
    pub username: String,
    #[serde(skip_serializing)]
    #[better_debug(ignore = true)]
    pub password: String,
    #[serde(skip_deserializing, default = "chrono::offset::Utc::now")]
    #[as_query(column = "Column::CreatedAt")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "is_superuser_default")]
    pub is_superuser: bool,
    #[serde(default = "active_default")]
    pub active: bool,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdatableModel {
    pub username: Option<String>,
    pub password: Option<String>,
    pub is_superuser: Option<bool>,
    pub active: Option<bool>,
}

fn is_superuser_default() -> bool {
    false
}

fn active_default() -> bool {
    true
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::api_key::Entity")]
    ApiKey,
}

impl ActiveModelBehavior for ActiveModel {}

impl Related<super::format::Entity> for Entity {
    // The final relation is Cake -> CakeFilling -> Filling
    fn to() -> RelationDef {
        super::format_entitlement::Relation::Format.def()
    }

    fn via() -> Option<RelationDef> {
        // The original relation is CakeFilling -> Cake,
        // after `rev` it becomes Cake -> CakeFilling
        Some(super::format_entitlement::Relation::User.def().rev())
    }
}

impl Related<super::api_key::Entity> for Entity {
    // The final relation is Cake -> CakeFilling -> Filling
    fn to() -> RelationDef {
        Relation::ApiKey.def()
    }
}
