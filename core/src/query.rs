use ::entity::{
    format,
    format::Entity as Format,
    format_entitlement::Entity as FormatEntitlement,
    format_entitlement::{self, Access, SearchModel as FormatEntitlementSearch},
    user,
    user::Entity as User,
};
use sea_orm::*;

pub struct FormatQuery;

impl FormatQuery {
    pub async fn find_by_id(db: &DbConn, id: i32) -> Result<Option<format::Model>, DbErr> {
        Format::find_by_id(id).one(db).await
    }

    /// Returns all available formats using pagination
    pub async fn get_all(
        db: &DbConn,
        search_params: format::ModelAsQuery,
        fetch_page: u64,
        page_size: u64,
    ) -> Result<(Vec<format::Model>, u64, u64), DbErr> {
        let select = search_params.filter(Format::find());
        let paginator = search_params.sort(select).paginate(db, page_size);
        let num_items = paginator.num_items().await?;
        let num_pages = (num_items as f64 / page_size as f64).ceil() as u64;
        paginator
            .fetch_page(fetch_page)
            .await
            .map(|p| (p, num_pages, num_items))
    }
}

pub struct UserQuery;

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

pub struct FormatEntitlementQuery;

impl FormatEntitlementQuery {
    pub async fn find_by_id(
        db: &DbConn,
        id: &FormatEntitlementSearch,
    ) -> Result<Option<format_entitlement::Model>, DbErr> {
        format_entitlement::Entity::find_by_id((id.user_id, id.format_id))
            .one(db)
            .await
    }

    pub async fn get_all(
        db: &DbConn,
        search_params: format_entitlement::ModelAsQuery,
        fetch_page: u64,
        page_size: u64,
    ) -> Result<(Vec<format_entitlement::Model>, u64, u64), DbErr> {
        // todo!()
        // println!("paginated: {}", format::Column::CreatedAt.to_string());

        let select = search_params.filter(FormatEntitlement::find());
        let paginator = search_params.sort(select).paginate(db, page_size);
        let num_items = paginator.num_items().await?;
        let num_pages = (num_items as f64 / page_size as f64).ceil() as u64;
        paginator
            .fetch_page(fetch_page)
            .await
            .map(|p| (p, num_pages, num_items))
    }
}
