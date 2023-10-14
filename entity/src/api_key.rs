use crate::traits::AsQueryParamFilterable;
use crate::traits::AsQueryParamSortable;
use central_repository_macros::AsQueryParam;
use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Default, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize, AsQueryParam,
)]
#[as_query(sort_default_column = "Column::Id", camel_case)]
#[sea_orm(table_name = "api_key")]
#[serde(rename_all = "camelCase")]
pub struct Model {
    // Don't let users define this field.
    #[serde(skip_deserializing)]
    #[sea_orm(primary_key, auto_increment = false)]
    #[as_query(column = "Column::Id", eq, custom_convert = "*value")]
    pub id: Uuid,
    #[as_query(column = "Column::UserId", eq, custom_convert = "*value")]
    pub user_id: Uuid,
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
    pub last_rotated_at: DateTime<Utc>,
    #[serde(default = "active_default")]
    #[as_query(column = "Column::Active", eq, custom_convert = "*value")]
    pub active: bool,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdatableModel {
    pub active: Option<bool>,
    // Not part of the real model, but this can be used
    // to indicate the wish to rotate this key.
    pub rotate: Option<bool>,
}

fn active_default() -> bool {
    true
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

// `Related` trait has to be implemented by hand
impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
