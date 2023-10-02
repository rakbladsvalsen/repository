/// GAT-related features (generic associated types) for models.
use crate::traits::*;
use async_trait::async_trait;
use futures::{try_join, Stream};
use log::{debug, info};
use sea_orm::*;
use sea_query::{Alias, Expr, SelectStatement};
use std::{fmt::Debug, pin::Pin};

type ResultModel<T> = Result<T, DbErr>;

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
    /// Get number of items and pages.
    /// There's already a built-in SeaORM method with the same name
    /// (num_items_and _pages) but it has a weird bug for some reason
    /// that causes the COUNT call to be terribly slow.
    ///
    /// See more: https://github.com/SeaQL/sea-orm/issues/1888
    ///
    /// Returns a tuple, containing:
    /// - (Total number of pages, total number of items)
    async fn num_items_and_pages<C: ConnectionTrait>(
        db: &C,
        select: &mut sea_orm::Select<Self::Entity>,
        page_size: u64,
    ) -> Result<(u64, u64), DbErr> {
        let select = SelectStatement::new()
            .expr(Expr::cust("COUNT (*) AS num_items"))
            .from_subquery(
                sea_orm::QueryTrait::query(select).to_owned(),
                Alias::new("sub_query"),
            )
            .to_owned();

        let stmt = StatementBuilder::build(&select, &sea_orm::DatabaseBackend::Postgres);

        let result = db.query_all(stmt).await?;
        let result = match result.get(0) {
            Some(i) => i,
            _ => return Ok((0, 0)),
        };
        let num_items = result.try_get::<i64>("", "num_items")?;
        let num_pages = (num_items as f64 / page_size as f64).ceil() as u64;
        Ok((num_pages, num_items as u64))
    }

    fn prepare_select(
        filters: &Self::FilterQueryModel,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Select<Self::Entity> {
        debug!("filter params: {:#?}", filters);
        let mut select_stmt = select_stmt.unwrap_or_else(Self::Entity::find);
        select_stmt = filters.filter(select_stmt);
        select_stmt = filters.sort(select_stmt);
        select_stmt
    }

    /// Get all available items as a stream.
    ///
    /// This will apply the query and return an unpaged stream of items.
    /// Parameters:
    ///
    /// `db`: The database connection.
    /// filtters: Filters to apply to the query.
    /// select_stmt: Optional statement to use for the query.
    async fn get_all_as_stream<C: ConnectionTrait + StreamTrait + 'db>(
        db: &'db C,
        filters: &Self::FilterQueryModel,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
    ) -> Result<Pin<Box<dyn Stream<Item = ResultModel<Self::ResultModel>> + Send + 'db>>, DbErr>
    {
        let select = Self::prepare_select(filters, select_stmt);
        Ok(select.stream(db).await.map(Box::pin)?)
    }

    /// Get all available items using pagination.
    async fn get_all<C: ConnectionTrait, O: IntoSimpleExpr + Send>(
        db: &C,
        filters: &Self::FilterQueryModel,
        pagination_options: &PaginationOptions,
        select_stmt: Option<sea_orm::Select<Self::Entity>>,
        order_by: O,
    ) -> Result<(Vec<Self::ResultModel>, u64, u64), DbErr> {
        debug!("pagination options: {:#?}", pagination_options);
        let mut select = Self::prepare_select(filters, select_stmt);
        let select_ordered = select.clone().order_by_asc(order_by);

        // Create paginators.
        // Note that the ordered paginator only returns items sorted by whatever column was passed in order_by,
        // for the actual count we don't need to ORDER BY the internal query.
        let paginator_ordered = select_ordered.paginate(db, pagination_options.page_size);
        let pagination_fut = paginator_ordered.fetch_page(pagination_options.fetch_page);

        if pagination_options.items_and_pages {
            info!("executing potentially slow query");
            // let paginator = select.paginate(db, pagination_options.page_size);
            let items_and_pages_fut =
                Self::num_items_and_pages(db, &mut select, pagination_options.page_size);
            // if items and pages is enabled, run two queries concurrently:
            // - a normal SELECT query
            // - a COUNT(*) query
            let (items, (num_pages, num_items)) = try_join!(pagination_fut, items_and_pages_fut)?;
            return Ok((items, num_pages, num_items));
        }
        // if items and pages is disabled, run a single query
        Ok((pagination_fut.await?, 0, 0))
    }
}

#[async_trait]
impl<'db, T> GetAllPaginated<'db> for T where T: GetAllTrait<'db> {}
