use crate::{pagination_impl::GetAllTrait, GetAllPaginated};
use ::entity::{
    format,
    format::Entity as Format,
    format_entitlement::{self, Access, SearchModel as FormatEntitlementSearch},
    record, upload_session, user,
    user::Entity as User,
};
use sea_orm::{sea_query::Query, *};

// Query objects
pub struct UploadSessionQuery;
pub struct FormatEntitlementQuery;
pub struct FormatQuery;
pub struct UserQuery;
pub struct RecordQuery;

// Implement get_all for all of them
impl GetAllTrait<'_> for FormatQuery {
    type Entity = format::Entity;
    type FilterQueryModel = format::ModelAsQuery;
    type ResultModel = format::Model;
}

impl GetAllTrait<'_> for FormatEntitlementQuery {
    type FilterQueryModel = format_entitlement::ModelAsQuery;
    type ResultModel = format_entitlement::Model;
    type Entity = format_entitlement::Entity;
}

impl GetAllTrait<'_> for UploadSessionQuery {
    type FilterQueryModel = upload_session::ModelAsQuery;
    type ResultModel = upload_session::Model;
    type Entity = upload_session::Entity;
}

impl GetAllTrait<'_> for RecordQuery {
    type FilterQueryModel = record::ModelAsQuery;
    type ResultModel = record::Model;
    type Entity = record::Entity;
}

// Custom impl's for weird usecases.

impl RecordQuery {
    /// Get all records where this user has read access
    pub async fn get_all_for_user<C: ConnectionTrait>(
        db: &C,
        filters: &record::ModelAsQuery,
        fetch_page: u64,
        page_size: u64,
        user: user::Model,
    ) -> Result<(Vec<record::Model>, u64, u64), DbErr> {
        // filter formats that this user can read (ReadWrite/ReadOnly)
        let available_read_formats = user
            .find_related(format::Entity)
            .select_only()
            .column(format::Column::Id) // just pull id
            .filter(Condition::all().add(
                format_entitlement::Column::Access.is_in([Access::ReadWrite, Access::ReadOnly]),
            ))
            .as_query()
            .to_owned();
        // filter upload sessions where the format ID is in the available
        // list of format IDs available for read
        let upload_session_filter = Query::select()
            .column(upload_session::Column::Id)
            .cond_where(upload_session::Column::FormatId.in_subquery(available_read_formats))
            .from(upload_session::Entity)
            .to_owned();
        // finally, filter records whose upload_session_id is in the above
        // upload_session ids (basically, filter records this user has read access)
        let select_stmt = record::Entity::find().filter(
            Condition::all()
                .add(record::Column::UploadSessionId.in_subquery(upload_session_filter)),
        );
        RecordQuery::get_all(db, filters, fetch_page, page_size, Some(select_stmt)).await
    }
}

impl FormatQuery {
    pub async fn find_by_id(db: &DbConn, id: i32) -> Result<Option<format::Model>, DbErr> {
        Format::find_by_id(id).one(db).await
    }
}

impl UserQuery {
    pub async fn find_by_id(db: &DbConn, id: i32) -> Result<Option<user::Model>, DbErr> {
        User::find_by_id(id).one(db).await
    }

    pub async fn find_nonsuperuser_by_id(
        db: &DbConn,
        id: i32,
    ) -> Result<Option<user::Model>, DbErr> {
        User::find_by_id(id)
            .filter(user::Column::IsSuperuser.eq(false))
            .one(db)
            .await
    }

    pub async fn find_by_username(
        db: &DbConn,
        username: &String,
    ) -> Result<Option<user::Model>, DbErr> {
        User::find()
            .filter(user::Column::Username.eq(username))
            .one(db)
            .await
    }

    /// Verify whether the passed user has write access to `fmt` (a format).
    pub async fn find_writable_format(
        db: &DbConn,
        user: &user::Model,
        format_id: i32,
    ) -> Result<Option<format::Model>, DbErr> {
        user.find_related(format::Entity)
            .filter(format::Column::Id.eq(format_id))
            .filter(
                // Filter only formats this user can write to
                Condition::any().add(
                    format_entitlement::Column::Access
                        .is_in([Access::ReadWrite, Access::WriteOnly]),
                ),
            )
            .one(db)
            .await
    }
}

impl FormatEntitlementQuery {
    pub async fn find_by_id(
        db: &DbConn,
        id: &FormatEntitlementSearch,
    ) -> Result<Option<format_entitlement::Model>, DbErr> {
        format_entitlement::Entity::find_by_id((id.user_id, id.format_id))
            .one(db)
            .await
    }
}
