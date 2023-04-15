/// GAT-related features (generic associated types) for models.
use crate::traits::*;
use async_trait::async_trait;
use log::debug;
use sea_orm::*;
use std::fmt::Debug;

pub trait GetAllTrait<'db> {
    type ResultModel: FromQueryResult + Sized + Send + Sync + 'db;
    type FilterQueryModel: AsQueryParamFilterable + AsQueryParamSortable + Debug + Send + Sync;
    type Entity: EntityTrait<Model = Self::ResultModel>;
}

#[async_trait]
pub trait GetAllPaginated<'db>: GetAllTrait<'db> {
    async fn get_all<C: ConnectionTrait>(
        db: &C,
        filters: &Self::FilterQueryModel,
        fetch_page: u64,
        page_size: u64,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Result<(Vec<Self::ResultModel>, u64, u64), DbErr> {
        let select_stmt = select_stmt.unwrap_or_else(Self::Entity::find);
        debug!("fetching page: {}, page_size: {}", fetch_page, page_size);
        debug!("filter params: {:?}", filters);
        let select = filters.sort(filters.filter(select_stmt));
        let paginator = select.paginate(db, page_size);
        let items_and_pages = paginator.num_items_and_pages().await?;
        paginator.fetch_page(fetch_page).await.map(|p| {
            (
                p,
                items_and_pages.number_of_pages,
                items_and_pages.number_of_items,
            )
        })
    }
}

#[async_trait]
impl<'db, T> GetAllPaginated<'db> for T where T: GetAllTrait<'db> {}
