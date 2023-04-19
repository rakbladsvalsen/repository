/// GAT-related features (generic associated types) for models.
use crate::traits::*;
use async_trait::async_trait;
use futures::join;
use log::{debug, info};
use sea_orm::*;
use std::fmt::Debug;

pub trait GetAllTrait<'db> {
    type ResultModel: FromQueryResult + Sized + Send + Sync + 'db;
    type FilterQueryModel: AsQueryParamFilterable + AsQueryParamSortable + Debug + Send + Sync;
    type Entity: EntityTrait<Model = Self::ResultModel>;
}

#[derive(Debug, Clone)]
pub struct PaginationOptions {
    /// The page to fetch.
    pub fetch_page: u64,
    /// The number of items per page.
    pub page_size: u64,
    /// Whether to fetch items and page count.
    pub items_and_pages: bool,
}

#[async_trait]
pub trait GetAllPaginated<'db>: GetAllTrait<'db> {
    async fn get_all<C: ConnectionTrait>(
        db: &C,
        filters: &Self::FilterQueryModel,
        pagination_options: &PaginationOptions,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Result<(Vec<Self::ResultModel>, u64, u64), DbErr> {
        let select_stmt = select_stmt.unwrap_or_else(Self::Entity::find);
        debug!("pagination options: {:#?}", pagination_options);
        debug!("filter params: {:#?}", filters);
        let select = filters.sort(filters.filter(select_stmt));
        let paginator = select.paginate(db, pagination_options.page_size);
        let pagination_fut = paginator.fetch_page(pagination_options.fetch_page);
        if pagination_options.items_and_pages {
            info!("executing potentially slow query");
            // if items and pages is enabled, run two queries concurrently:
            // - a normal SELECT query
            // - a COUNT(*) query
            let (paginated, items_and_pages) =
                join!(pagination_fut, paginator.num_items_and_pages());
            let (paginated, items_and_pages) = (paginated?, items_and_pages?);
            return Ok((
                paginated,
                items_and_pages.number_of_pages,
                items_and_pages.number_of_items,
            ));
        }
        // if items and pages is disabled, run a single query
        Ok((pagination_fut.await?, 0, 0))
    }
}

#[async_trait]
impl<'db, T> GetAllPaginated<'db> for T where T: GetAllTrait<'db> {}
