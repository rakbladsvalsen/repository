use std::ops::Deref;

use crate::traits::{AsQueryParamFilterable, AsQueryParamSortable};
use central_repository_macros::AsQueryParam;
use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use sea_orm::Select;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColumnKind {
    Number,
    String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ColumnSchema {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    pub kind: ColumnKind,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, FromJsonQueryResult)]
pub struct FormatSchema(pub Vec<ColumnSchema>);

impl Deref for FormatSchema {
    type Target = Vec<ColumnSchema>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(AsQueryParam, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "format")]
#[serde(rename_all = "camelCase")]
#[as_query(sort_default_column = "Column::Id", camel_case)]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[serde(skip_deserializing)]
    #[as_query(column = "Column::Id", eq, lt, gt, lte, gte, custom_convert = "*value")]
    pub id: i32,
    #[as_query(column = "Column::Name", eq, contains, like)]
    pub name: String,
    #[sea_orm(column_type = "Text")]
    pub description: String,
    #[serde(skip_deserializing)]
    #[as_query(column = "Column::CreatedAt")]
    pub created_at: DateTime<Utc>,
    pub schema: FormatSchema,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        super::format_entitlement::Relation::User.def()
    }

    fn via() -> Option<RelationDef> {
        Some(super::format_entitlement::Relation::Format.def().rev())
    }
}
