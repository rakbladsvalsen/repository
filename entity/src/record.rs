use std::collections::HashMap;

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Document type used throughout the entire project.
/// Note that the JSON value can be any JSON object, though
/// this will most likely be either a number or a string,
/// as defined by the Format.
pub type DynamicHashmap = HashMap<String, Value>;

#[derive(Default, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, FromJsonQueryResult)]
pub struct RecordJsonData(pub DynamicHashmap);

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize, Default)]
#[sea_orm(table_name = "record")]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[serde(skip_deserializing)]
    pub id: i32,
    pub upload_session_id: i32,
    pub data: RecordJsonData,
}

impl Model {
    pub fn new(upload_session_id: i32, data: DynamicHashmap) -> Self {
        Self {
            upload_session_id,
            data: RecordJsonData(data),
            ..Default::default()
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    // define inverse relation
    #[sea_orm(
        belongs_to = "super::upload_session::Entity",
        from = "Column::Id",
        to = "super::upload_session::Column::Id"
    )]
    UploadSession,
}

impl Related<super::upload_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UploadSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
