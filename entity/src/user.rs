use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "user")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[serde(skip_deserializing)]
    pub id: i32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    #[serde(skip_deserializing, default = "chrono::offset::Utc::now")]
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
