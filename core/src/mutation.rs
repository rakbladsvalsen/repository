use ::entity::{
    format,
    format::Entity as Format,
    format_entitlement, record,
    record::Entity as Record,
    upload_session::{self, OutcomeKind},
    user,
};
use sea_orm::*;

pub struct FormatMutation;

impl FormatMutation {
    pub async fn create(db: &DbConn, model: format::Model) -> Result<format::ActiveModel, DbErr> {
        format::ActiveModel {
            name: Set(model.name),
            description: Set(model.description),
            created_at: Set(chrono::offset::Utc::now()),
            schema: Set(model.schema),
            ..Default::default()
        }
        .save(db)
        .await
    }

    pub async fn delete(db: &DbConn, id: i32) -> Result<DeleteResult, DbErr> {
        let format: format::ActiveModel = Format::find_by_id(id)
            .one(db)
            .await?
            .ok_or(DbErr::RecordNotFound("format".into()))
            .map(Into::into)?;

        format.delete(db).await
    }
}

pub struct UploadSessionMutation;
impl UploadSessionMutation {
    pub async fn create(
        db: &DbConn,
        model: upload_session::Model,
    ) -> Result<upload_session::Model, DbErr> {
        let mut model = model.into_active_model();
        model.id = NotSet;
        model.created_at = Set(chrono::offset::Utc::now());
        model.insert(db).await
    }

    pub async fn update_as_failed<I: Into<i32>, S: Into<String>>(
        db: &DbConn,
        upload_session_id: I,
        detail: S,
    ) -> Result<(), DbErr> {
        let session = upload_session::Entity::find_by_id(upload_session_id)
            .one(db)
            .await?;
        match session {
            Some(found) => {
                let mut found = found.into_active_model();
                found.outcome = Set(OutcomeKind::Error);
                found.detail = Set(detail.into());
                found.update(db).await.map(|_| Ok(()))?
            }
            _ => Err(DbErr::RecordNotFound("Not found".into())),
        }
    }
}

pub struct RecordMutation;
impl RecordMutation {
    pub async fn create_many<I>(db: &DbConn, entries: I) -> Result<u64, DbErr>
    where
        I: IntoIterator<Item = record::Model>,
    {
        let converted = entries.into_iter().map(|entry| record::ActiveModel {
            upload_session_id: Set(entry.upload_session_id),
            data: Set(entry.data),
            ..Default::default()
        });
        Record::insert_many(converted)
            .exec_without_returning(db)
            .await
    }
}

pub struct UserMutation;
impl UserMutation {
    pub async fn create(db: &DbConn, user: user::Model) -> Result<user::Model, DbErr> {
        let mut user = user::ActiveModel::from(user);
        user.id = NotSet;
        user.insert(db).await
    }
}

pub struct FormatEntitlementMutation;

impl FormatEntitlementMutation {
    pub async fn create(
        db: &DbConn,
        model: format_entitlement::Model,
    ) -> Result<format_entitlement::Model, DbErr> {
        format_entitlement::ActiveModel::from(model)
            .insert(db)
            .await
    }
}
