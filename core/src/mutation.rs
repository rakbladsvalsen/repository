use ::entity::{
    api_key,
    error::DatabaseQueryError,
    format,
    format::{ColumnKind, Entity as Format},
    format_entitlement, record,
    record::Entity as Record,
    upload_session::{self, OutcomeKind},
    user,
};
use log::info;
use regex::Regex;
use sea_orm::*;
use uuid::Uuid;

pub struct FormatMutation;

impl FormatMutation {
    pub async fn create(
        db: &DbConn,
        model: format::Model,
    ) -> Result<format::ActiveModel, DatabaseQueryError> {
        let is_regex_invalid = model.schema.iter().filter(|i| i.regex.is_some()).any(|i| {
            // only string columns can be checked against a regex
            i.kind != ColumnKind::String || Regex::new(i.regex.as_ref().unwrap().as_str()).is_err()
        });
        if is_regex_invalid {
            return Err(DatabaseQueryError::InvalidRegex);
        }

        format::ActiveModel {
            name: Set(model.name),
            description: Set(model.description),
            created_at: Set(chrono::offset::Utc::now()),
            schema: Set(model.schema),
            ..Default::default()
        }
        .save(db)
        .await
        .map_err(Into::into)
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
    #[inline(always)]
    pub async fn create_many<I>(db: &DbConn, entries: I) -> Result<u64, DbErr>
    where
        I: IntoIterator<Item = record::Model>,
    {
        let converted = entries.into_iter().map(|entry| record::ActiveModel {
            upload_session_id: Set(entry.upload_session_id),
            format_id: Set(entry.format_id),
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
        user.id = Set(Uuid::new_v4());
        user.insert(db).await
    }

    pub async fn update(
        db: &DbConn,
        old_user: user::Model,
        new_user: user::UpdatableModel,
    ) -> Result<user::Model, DbErr> {
        let mut user = old_user.into_active_model();
        user.username = new_user.username.map(Set).unwrap_or(NotSet);
        user.password = new_user.password.map(Set).unwrap_or(NotSet);
        user.is_superuser = new_user.is_superuser.map(Set).unwrap_or(NotSet);
        user.active = new_user.active.map(Set).unwrap_or(NotSet);
        user.update(db).await
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

pub struct ApiKeyMutation;

impl ApiKeyMutation {
    pub async fn delete(db: &DbConn, model: api_key::Model) -> Result<(), DbErr> {
        api_key::Entity::delete_by_id(model.id)
            .exec(db)
            .await
            .map(|_| ())
    }

    /// Create an API Key for this user.
    pub async fn create_for_user(db: &DbConn, user: &user::Model) -> Result<api_key::Model, DbErr> {
        let now = chrono::offset::Utc::now();
        api_key::ActiveModel {
            user_id: Set(user.id),
            created_at: Set(now),
            last_rotated_at: Set(now),
            active: Set(true),
            id: Set(Uuid::new_v4()),
        }
        .insert(db)
        .await?
        .try_into_model()
    }

    pub async fn update(
        db: &DbConn,
        old: api_key::Model,
        new: api_key::UpdatableModel,
    ) -> Result<api_key::Model, DbErr> {
        let mut model = old.into_active_model();
        // User enabled 'rotate' option, so let's just rotate this api key.
        if new.rotate.unwrap_or(false) {
            let now = chrono::offset::Utc::now();
            info!("rotating key with ID: {:?}: {:?}", model.id, now);
            model.last_rotated_at = Set(now);
        }
        model.active = new.active.map(Set).unwrap_or(NotSet);
        model.update(db).await
    }
}
