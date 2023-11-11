use ::entity::{
    api_key,
    error::DatabaseQueryError,
    format,
    format::{ColumnKind, Entity as Format},
    format_entitlement::{self, AccessLevel, ARRAY_CONTAINS_OP},
    record,
    record::Entity as Record,
    upload_session::{self, OutcomeKind},
    user,
};
use central_repository_config::inner::Config;
use log::{debug, info};
use regex::Regex;
use sea_orm::*;
use sea_query::Expr;
use uuid::Uuid;

use crate::conf::DBConfig;

pub struct FormatMutation;

impl FormatMutation {
    pub async fn create(model: format::Model) -> Result<format::ActiveModel, DatabaseQueryError> {
        let db = DBConfig::get_connection();
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

    pub async fn delete(id: i32) -> Result<DeleteResult, DbErr> {
        let db = DBConfig::get_connection();
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
    pub async fn create(model: upload_session::Model) -> Result<upload_session::Model, DbErr> {
        let db = DBConfig::get_connection();
        let mut model = model.into_active_model();
        model.id = NotSet;
        model.created_at = Set(chrono::offset::Utc::now());
        model.insert(db).await
    }

    pub async fn update_as_failed<I: Into<i32>, S: Into<String>>(
        upload_session_id: I,
        detail: S,
    ) -> Result<(), DbErr> {
        let db = DBConfig::get_connection();
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

    #[inline]
    pub async fn delete(user: user::Model, id: i32) -> Result<(), DatabaseQueryError> {
        match user.is_superuser {
            true => Self::delete_by_id(id).await,
            false => Self::delete_non_superuser(user, id).await,
        }
    }

    #[inline(always)]
    pub async fn delete_non_superuser(
        user: user::Model,
        id: i32,
    ) -> Result<(), DatabaseQueryError> {
        let db = DBConfig::get_connection();
        let user_formats = format_entitlement::Entity::find()
            .filter(format_entitlement::Column::UserId.eq(user.id));
        let user_formats_subquery = user_formats
            .clone()
            .select_only()
            .column(format_entitlement::Column::FormatId);
        let formats_for_user_query = user_formats_subquery.as_query();
        let upload_session = upload_session::Entity::find()
            .filter(upload_session::Column::Id.eq(id))
            .filter(upload_session::Column::FormatId.in_subquery(formats_for_user_query.to_owned()))
            .one(db)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("upload session".into()))?;
        debug!("Delete: {upload_session:?}");
        // We need to get the entitlement anyway to check if the user actually
        // has the ability to delete this session.
        let col = Expr::col(format_entitlement::Column::Access);
        // Get entitlement
        let has_delete_access_filter = Condition::any()
            .add(col.clone().binary(
                ARRAY_CONTAINS_OP,
                AccessLevel::LimitedDelete.get_serialized().as_str(),
            ))
            .add(col.clone().binary(
                ARRAY_CONTAINS_OP,
                AccessLevel::Delete.get_serialized().as_str(),
            ));

        let entitlement = user_formats
            .filter(format_entitlement::Column::FormatId.eq(upload_session.format_id))
            .filter(has_delete_access_filter)
            .one(db)
            .await?
            .ok_or_else(|| {
                info!(
                    "User {} doesn't have an entitlement for format {}. Can't delete upload.",
                    user.username, upload_session.format_id
                );
                DatabaseQueryError::InsufficientPermissions
            })?;

        // Users with `Delete` permission can delete any upload session, no
        // matter when it was created.
        if entitlement.access.contains(&AccessLevel::Delete) {
            Self::delete_by_id(id).await?;
        } else if entitlement.access.contains(&AccessLevel::LimitedDelete) {
            let now = chrono::offset::Utc::now();
            let delta = now - upload_session.created_at;
            if delta > chrono::Duration::hours(Config::get().temporal_delete_hours as i64) {
                info!(
                    "User {} tried to delete an old upload {}: {delta}",
                    user.username, upload_session.id
                );
                return Err(DatabaseQueryError::InsufficientPermissions);
            }
            Self::delete_by_id(id).await?;
        }
        Ok(())
    }

    pub async fn delete_by_id(id: i32) -> Result<(), DatabaseQueryError> {
        let db = DBConfig::get_connection();
        let result = upload_session::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected != 1 {
            return Err(DatabaseQueryError::from(DbErr::RecordNotFound(format!(
                "upload session with id '{id}'"
            ))));
        }
        Ok(())
    }
}

pub struct RecordMutation;
impl RecordMutation {
    #[inline(always)]
    pub async fn create_many<I>(entries: I) -> Result<u64, DbErr>
    where
        I: IntoIterator<Item = record::Model>,
    {
        let db = DBConfig::get_connection();
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
    pub async fn create(user: user::Model) -> Result<user::Model, DbErr> {
        let mut user = user::ActiveModel::from(user);
        let db = DBConfig::get_connection();
        user.id = Set(Uuid::new_v4());
        user.insert(db).await
    }

    pub async fn update(
        old_user: user::Model,
        new_user: user::UpdatableModel,
    ) -> Result<user::Model, DbErr> {
        let db = DBConfig::get_connection();
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
        model: format_entitlement::Model,
    ) -> Result<format_entitlement::Model, DbErr> {
        let db = DBConfig::get_connection();
        format_entitlement::ActiveModel::from(model)
            .insert(db)
            .await
    }
}

pub struct ApiKeyMutation;

impl ApiKeyMutation {
    pub async fn delete(model: api_key::Model) -> Result<(), DbErr> {
        let db = DBConfig::get_connection();
        api_key::Entity::delete_by_id(model.id)
            .exec(db)
            .await
            .map(|_| ())
    }

    /// Create an API Key for this user.
    pub async fn create_for_user(user: &user::Model) -> Result<api_key::Model, DbErr> {
        let db = DBConfig::get_connection();
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
        old: api_key::Model,
        new: api_key::UpdatableModel,
    ) -> Result<api_key::Model, DbErr> {
        let db = DBConfig::get_connection();
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
