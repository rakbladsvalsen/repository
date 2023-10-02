use crate::traits::{AsQueryParamFilterable, AsQueryParamSortable};
use central_repository_macros::AsQueryParam;
use sea_orm::{entity::prelude::*, FromJsonQueryResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, ops::Deref};

/// Document type used throughout the entire project.
/// Note that the JSON value can be any JSON object, though
/// this will most likely be either a number or a string,
/// as defined by the Format.
pub type DynamicHashmap = HashMap<String, Value>;

#[derive(Default, Serialize, Deserialize, Debug, Clone, PartialEq, Eq, FromJsonQueryResult)]
pub struct RecordJsonData(pub DynamicHashmap);

impl Deref for RecordJsonData {
    type Target = DynamicHashmap;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(
    AsQueryParam, Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize, Default,
)]
#[sea_orm(table_name = "record")]
#[as_query(sort_default_column = "Column::Id", camel_case)]
pub struct Model {
    #[sea_orm(primary_key)]
    // Don't let users define this field.
    #[as_query(column = "Column::Id", eq, lt, gt, lte, gte, custom_convert = "*value")]
    #[serde(skip_deserializing)]
    pub id: i64,
    #[as_query(
        column = "Column::UploadSessionId",
        eq,
        lt,
        gt,
        lte,
        gte,
        custom_convert = "*value"
    )]
    #[serde(skip_deserializing)]
    pub upload_session_id: i32,
    #[serde(skip_deserializing)]
    pub format_id: i32,
    pub data: RecordJsonData,
}

impl Model {
    #[inline(always)]
    pub fn new(upload_session_id: i32, format_id: i32, data: DynamicHashmap) -> Self {
        Self {
            upload_session_id,
            format_id,
            data: RecordJsonData(data),
            id: Default::default(),
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
    #[sea_orm(
        belongs_to = "super::format::Entity",
        from = "Column::Id",
        to = "super::format::Column::Id"
    )]
    FormatId,
    #[sea_orm(
        belongs_to = "super::record::Entity",
        from = "Column::Data",
        to = "Column::Data"
    )]
    Data,
}

impl Related<super::upload_session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UploadSession.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
