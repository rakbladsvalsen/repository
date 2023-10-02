use actix_web::HttpResponse;
use central_repository_dao::PaginationOptions;
use log::info;
use serde::{Deserialize, Serialize};

use crate::{
    conf::{DEFAULT_PAGINATION_SIZE, MAX_PAGINATION_SIZE, RETURN_QUERY_COUNT},
    error::APIError,
};

fn default_page() -> u64 {
    0
}

fn default_page_size() -> u64 {
    *DEFAULT_PAGINATION_SIZE
}

fn default_full_count() -> bool {
    *RETURN_QUERY_COUNT
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct APIPager {
    /// The page to fetch.
    #[serde(default = "default_page")]
    pub page: u64,
    /// The number of items per page.
    #[serde(default = "default_page_size")]
    pub per_page: u64,
    /// Whether to fetch items and page count.
    #[serde(default = "default_full_count")]
    pub count: bool,
}

impl From<APIPager> for PaginationOptions {
    fn from(pager: APIPager) -> Self {
        PaginationOptions {
            fetch_page: pager.page,
            page_size: pager.per_page,
            items_and_pages: pager.count,
        }
    }
}

impl APIPager {
    pub fn validate(&self) -> Result<(), APIError> {
        if self.per_page == 0 {
            return Err(APIError::InvalidPageSize(
                "per_page must be greater than 0".to_string(),
            ));
        } else if self.per_page > *MAX_PAGINATION_SIZE {
            return Err(APIError::InvalidPageSize(format!(
                "per_page must be less than {}",
                *MAX_PAGINATION_SIZE
            )));
        }
        Ok(())
    }
}

pub struct PaginatedResponse<T> {
    items: Vec<T>,
    num_pages: u64,
    num_items: u64,
}

// From<> for load_and_count_pages's output
impl<T> From<(Vec<T>, u64, u64)> for PaginatedResponse<T>
where
    T: Serialize,
{
    fn from(data: (Vec<T>, u64, u64)) -> PaginatedResponse<T> {
        let (items, num_pages, num_items) = data;
        PaginatedResponse {
            items,
            num_pages,
            num_items,
        }
    }
}

impl<T> From<PaginatedResponse<T>> for HttpResponse
where
    T: Serialize,
{
    fn from(value: PaginatedResponse<T>) -> Self {
        info!(
            "page count: {}, item count: {}, returning {} items",
            value.num_pages,
            value.num_items,
            value.items.len()
        );
        HttpResponse::Ok()
            .insert_header(("repository-item-count", value.num_items))
            .insert_header(("repository-current-page-count", value.items.len()))
            .insert_header(("repository-page-count", value.num_pages))
            .json(value.items)
    }
}
