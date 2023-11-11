use actix_web::HttpResponse;
use central_repository_dao::PaginationOptions;
use log::info;
use serde::Serialize;

use crate::error::APIError;

pub trait Validate {
    fn validate(&self) -> Result<(), APIError>;
}

impl Validate for PaginationOptions {
    fn validate(&self) -> Result<(), APIError> {
        if !self.is_valid() {
            return Err(APIError::InvalidPaginationParameters(
                "Pagination parameters are invalid".to_string(),
            ));
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
