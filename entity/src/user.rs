use crate::traits::AsQueryParamFilterable;
use crate::traits::AsQueryParamSortable;
use central_repository_macros::AsQueryParam;
use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::extension::postgres::PgBinOper;
use sea_orm::sea_query::Expr;
use serde::{Deserialize, Serialize};

#[derive(
    Default, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize, AsQueryParam,
)]
#[as_query(sort_default_column = "Column::Id", camel_case)]
#[sea_orm(table_name = "user")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[as_query(column = "Column::Id", eq, lt, gt, lte, gte, custom_convert = "*value")]
    #[serde(skip_deserializing)]
    pub id: i32,
    #[as_query(column = "Column::Username", eq, like, ilike, contains)]
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    #[serde(skip_deserializing, default = "chrono::offset::Utc::now")]
    #[as_query(column = "Column::CreatedAt")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "is_superuser_default")]
    pub is_superuser: bool,
}

fn is_superuser_default() -> bool {
    false
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

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
