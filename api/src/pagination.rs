use actix_web::HttpResponse;
use log::debug;
use serde::{Deserialize, Serialize};
use validator::Validate;

const DEFAULT_POSTS_PER_PAGE: u64 = 100;

#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct APIPager {
    pub page: u64,
    #[validate(range(min = 1, max = 1000))]
    pub per_page: u64,
}

impl Default for APIPager {
    fn default() -> Self {
        APIPager {
            page: 0,
            per_page: DEFAULT_POSTS_PER_PAGE,
        }
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
        debug!(
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
