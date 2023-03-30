use sea_orm::{EntityTrait, Select};

pub trait AsQueryParamFilterable {
    fn filter<E: EntityTrait>(&self, select: Select<E>) -> Select<E>;
}

pub trait AsQueryParamSortable {
    fn sort<E: EntityTrait>(&self, select: Select<E>) -> Select<E>;
}
