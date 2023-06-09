use std::collections::{HashMap, HashSet};

use entity::{
    error::DatabaseQueryError,
    format::{self, ColumnKind},
    format_entitlement::{self, Access},
    record,
    traits::AsQueryParamFilterable,
    upload_session, user,
};
use itertools::Itertools;
use log::{debug, info};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use sea_orm::{
    sea_query::{extension::postgres::PgBinOper, BinOper, Expr, Query},
    ColumnTrait, Condition, DbConn, EntityTrait, ModelTrait, QueryFilter, QuerySelect, QueryTrait,
};
use serde::*;
use serde_json::Value;
// use entity::serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Proxy for sea_query's supported condition types.
pub enum ConditionKind {
    Any,
    All,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
/// Supported comparison operators.
pub enum ComparisonOperator {
    Eq,
    Lt,
    Gt,
    Lte,
    Gte,
    In,
    ILike,
    Like,
    Regex,
    RegexCaseInsensitive,
}

impl Default for ComparisonOperator {
    fn default() -> Self {
        Self::Eq
    }
}

impl Default for ConditionKind {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
/// A single search argument. This basically allows
/// users to define matches against a specific column.
pub struct SearchArguments {
    column: String,
    comparison_operator: ComparisonOperator,
    compare_against: serde_json::Value,
}

impl SearchArguments {
    fn validate_array(
        &self,
        predicate: fn(&serde_json::Value) -> bool,
        message: &str,
    ) -> Result<(), DatabaseQueryError> {
        let all_members_valid = self
            .compare_against
            .as_array()
            .ok_or(DatabaseQueryError::InvalidUsage(
                "comparison value isn't an array".into(),
            ))?
            .par_iter()
            .all(predicate);
        if !all_members_valid {
            return Err(DatabaseQueryError::InvalidUsage(message.into()));
        }
        Ok(())
    }

    fn validate_string(&self) -> Result<(), DatabaseQueryError> {
        match self.comparison_operator {
            ComparisonOperator::In => {
                self.validate_array(|i| i.is_string(), "one or more items isn't a string")
            }
            ComparisonOperator::Eq
            | ComparisonOperator::Like
            | ComparisonOperator::ILike
            | ComparisonOperator::Regex
            | ComparisonOperator::RegexCaseInsensitive => match self.compare_against.is_string() {
                true => Ok(()),
                _ => Err(DatabaseQueryError::InvalidUsage(
                    "cannot use this operator on non-string types".into(),
                )),
            },
            _ => Err(DatabaseQueryError::InvalidUsage(
                "cannot use numeric operator on strings".into(),
            )),
        }
    }

    fn validate_number(&self) -> Result<(), DatabaseQueryError> {
        match self.comparison_operator {
            ComparisonOperator::In => {
                self.validate_array(|i| i.is_number(), "one or more items isn't a number")
            }
            ComparisonOperator::Eq
            | ComparisonOperator::Gt
            | ComparisonOperator::Lt
            | ComparisonOperator::Lte
            | ComparisonOperator::Gte => match self.compare_against.is_number() {
                true => Ok(()),
                _ => Err(DatabaseQueryError::InvalidUsage(format!(
                    "'{}' can only be compared against numbers.",
                    self.column
                ))),
            },
            _ => Err(DatabaseQueryError::InvalidUsage(format!(
                "'{}' is numeric; you can only use numeric operators.",
                self.column
            ))),
        }
    }

    pub fn validate(&self, db_column_kind: &ColumnKind) -> Result<(), DatabaseQueryError> {
        match db_column_kind {
            ColumnKind::Number => self.validate_number(),
            ColumnKind::String => self.validate_string(),
        }
    }

    /// Whether this search argument is valid or not.
    /// We allow arbitrary json values such as objects,
    /// maps, lists, etc, but we can only compare against
    /// strings and numbers.
    pub fn is_valid(&self) -> bool {
        matches!(
            self.compare_against,
            serde_json::Value::Number(_)
                | serde_json::Value::String(_)
                | serde_json::Value::Array(_)
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
/// A container for multiple search groups.
pub struct SearchGroup {
    // Whether this statement should be negated, i.e. NOT (COLUMN_A EQ 123 AND COLUMN_B EQ 456)
    #[serde(default)]
    not: bool,
    // The operator that should be applied to the next statement
    #[serde(default)]
    condition_kind: ConditionKind,
    args: Vec<SearchArguments>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    // Optional list of formats to filter from. If left undefined:
    // - Admin users get data from all formats.
    // - Non-admin users get data from all available read/readWrite formats.
    formats: Option<Vec<i32>>,
    // Optional upload session filters.
    // The model as query provides a lot of knobs to search.
    upload_session: Option<upload_session::ModelAsQuery>,
    query: Vec<SearchGroup>,
}

#[derive(Debug, Clone)]
pub struct PreparedSearchQuery<'a> {
    available_read_formats: Vec<i32>,
    requested_search_columns: HashMap<&'a String, ColumnKind>,
    query: &'a SearchQuery,
}

impl SearchQuery {
    pub fn validate(&self) -> Result<(), DatabaseQueryError> {
        // validate the query vec isn't empty.
        // if the list of formats is defined, ensure it isn't empty.
        if self.formats.as_ref().is_some() && self.formats.as_ref().unwrap().is_empty() {
            return Err(DatabaseQueryError::EmptyQuery);
        }
        Ok(())
    }

    pub async fn get_readable_formats_for_user(
        &self,
        user: &user::Model,
        db: &DbConn,
    ) -> Result<PreparedSearchQuery, DatabaseQueryError> {
        // create list of all the requested search columns, i.e.
        // args: [{"column": "blah", "columnB": "blah2", "columnC": 123} ...].
        // This will allow us to validate any potential invalid queries, i.e.
        // filtering numbers against strings and so on.
        let filtered_user_columns = self
            .query
            .iter()
            // iterate over all search groups
            .map(|search_group| &search_group.args)
            // and then iterate over all group args and extract the requested column and value to compare against.
            .flat_map(|args| args.iter().map(|arg| (&arg.column, arg)))
            .collect::<HashMap<_, _>>();

        debug!(
            "requested search columns: {:?}",
            filtered_user_columns.keys()
        );

        // filter formats for this user.
        let mut filtered_formats = match user.is_superuser {
            // do not restrict available formats for superusers
            true => format::Entity::find(),
            // restrict available readable formats for non-superusers
            false => user
                .find_related(format::Entity)
                .filter(Condition::all().add(
                    format_entitlement::Column::Access.is_in([Access::ReadWrite, Access::ReadOnly]),
                )),
        };

        // if the user passed a list of formats to filter by, then
        // refine the search even further.
        if let Some(formats) = &self.formats {
            filtered_formats = filtered_formats.filter(format::Column::Id.is_in(formats.clone()));
        }

        let formats = filtered_formats.all(db).await?;
        let available_read_formats = formats.par_iter().map(|f| f.id).collect::<Vec<_>>();

        debug!(
            "selected/available read formats: {:?}",
            available_read_formats
        );

        let filterable_columns = formats
            .into_par_iter()
            .flat_map(|format| format.schema.0.into_par_iter())
            .filter_map(|db_column_schema| {
                // filter through the schemas of all available formats in parallel
                // and only pick those that contain the requested search columns.
                filtered_user_columns
                    .get(&db_column_schema.name)
                    .map(|search_args| (db_column_schema, search_args))
            })
            .collect::<Vec<_>>();

        // verify that all columns being searched have the same data type.
        // this can be an issue when searching across multiple formats with identical column names
        // but different types.
        let unique_column_types = filterable_columns
            .par_iter()
            .map(|(db_column_schema, _)| (&db_column_schema.name, &db_column_schema.kind))
            .collect::<HashSet<_>>();

        let mut unique_column_names = HashSet::new();
        let multitype_columns = unique_column_types
            .iter()
            .filter_map(|(name, _)| (!unique_column_names.insert(name)).then_some(name.as_str()))
            .take(5)
            .collect::<HashSet<_>>();

        if !multitype_columns.is_empty() {
            return Err(DatabaseQueryError::ColumnWithMixedTypesError(
                multitype_columns.into_iter().join(","),
            ));
        }

        // finally, make sure user-entered data matches the database type,
        // i.e. if the user entered a string, we can only search for strings.
        let requested_search_columns = filterable_columns
            .into_par_iter()
            .map(|(db_column_schema, search_args)| {
                search_args
                    .validate(&db_column_schema.kind)
                    .map(|_| (&search_args.column, db_column_schema.kind))
            })
            .collect::<Result<HashMap<_, _>, DatabaseQueryError>>()?;

        info!("requested column types: {:#?}", requested_search_columns);
        Ok(PreparedSearchQuery {
            available_read_formats,
            requested_search_columns,
            query: self,
        })
    }
}

impl PreparedSearchQuery<'_> {
    /// Limit the available visible records.
    fn limit_visible_records(&self) -> Condition {
        let subquery = Query::select()
            .column(upload_session::Column::Id)
            .cond_where(upload_session::Column::FormatId.is_in(self.available_read_formats.clone()))
            .from(upload_session::Entity)
            .to_owned();
        Condition::all().add(record::Column::UploadSessionId.in_subquery(subquery))
    }

    fn apply_upload_session_filters(&self) -> Option<Condition> {
        match self.query.upload_session.as_ref() {
            Some(upload_session_filters) => {
                debug!(
                    "applying upload session filters: {:?}",
                    upload_session_filters
                );
                let mut subquery = upload_session::Entity::find()
                    .select_only()
                    .column(upload_session::Column::Id);
                subquery = upload_session_filters.filter(subquery);
                let condition = Query::select()
                    .columns([upload_session::Column::Id])
                    .cond_where(upload_session::Column::Id.in_subquery(subquery.as_query().clone()))
                    .from(upload_session::Entity)
                    .to_owned();
                Some(Condition::all().add(record::Column::UploadSessionId.in_subquery(condition)))
            }
            _ => None,
        }
    }

    pub fn build_condition(self) -> Result<Condition, DatabaseQueryError> {
        // create extra filtering condition to search inside ALL JSONB hashmaps
        // upload_session.
        let mut extra_condition = Condition::all();
        extra_condition = extra_condition.add(self.limit_visible_records());
        // apply upload session filters, if any was passed.
        extra_condition = match self.apply_upload_session_filters() {
            Some(c) => extra_condition.add(c),
            _ => extra_condition,
        };

        for search_group in self.query.query.iter() {
            // iterate over all search groups and create conditions for each one
            let mut condition = match search_group.condition_kind {
                ConditionKind::All => Condition::all(),
                ConditionKind::Any => Condition::any(),
            };
            // iterate over all expressions inside this group
            for expression in search_group.args.iter() {
                // TODO: Change this to PgBinOper once it gets stabilized
                // let mut target_json_column = Expr::col(record::Column::Data)
                //     .binary(PgBinOper::CastJsonField, Expr::val(&expression.column));
                let mut target_json_column = Expr::cust_with_exprs(
                    "$1 ->> $2",
                    [
                        Expr::col(record::Column::Data).into(),
                        Expr::val(&expression.column).into(),
                    ],
                );

                // Get the database type for this column.
                let against_column_kind = self
                    .requested_search_columns
                    .get(&expression.column)
                    .ok_or_else(|| {
                        info!("couldn't find column kind for column {}", expression.column);
                        DatabaseQueryError::InvalidColumnRequested(expression.column.clone())
                    })?;

                // determine whether this filter needs cast to FLOAT (postgres needs it)
                let cast_as_f64 = |value: &serde_json::Value| -> Result<f64, DatabaseQueryError> {
                    value.as_f64().ok_or_else(|| DatabaseQueryError::CastError)
                };
                target_json_column = match against_column_kind {
                    ColumnKind::String => target_json_column,
                    ColumnKind::Number => {
                        target_json_column.cast_as(sea_orm::sea_query::Alias::new("FLOAT"))
                    }
                };

                target_json_column = match expression.comparison_operator {
                    ComparisonOperator::In => {
                        let array = expression.compare_against.as_array().unwrap();
                        match against_column_kind {
                            ColumnKind::Number => {
                                let casted =
                                    PreparedSearchQuery::cast_value_array(array, |v| v.as_f64())?;
                                Expr::expr(target_json_column).is_in(casted)
                            }
                            ColumnKind::String => {
                                let casted =
                                    PreparedSearchQuery::cast_value_array(array, |v| v.as_str())?;
                                Expr::expr(target_json_column).is_in(casted)
                            }
                        }
                    }
                    ComparisonOperator::Regex => Expr::cust_with_exprs(
                        // use weird postgres regex operator (case sensitive)
                        "$1 ~ $2",
                        [
                            target_json_column,
                            expression.compare_against.as_str().into(),
                        ],
                    ),
                    ComparisonOperator::RegexCaseInsensitive => Expr::cust_with_exprs(
                        "$1 ~* $2",
                        [
                            target_json_column,
                            expression.compare_against.as_str().into(),
                        ],
                    ),
                    ComparisonOperator::ILike => target_json_column
                        .binary(PgBinOper::ILike, expression.compare_against.as_str()),
                    ComparisonOperator::Like => target_json_column
                        .binary(BinOper::Like, expression.compare_against.as_str()),
                    ComparisonOperator::Eq => match against_column_kind {
                        ColumnKind::Number => target_json_column
                            .binary(BinOper::Equal, expression.compare_against.as_f64()),
                        ColumnKind::String => target_json_column
                            .binary(BinOper::Equal, expression.compare_against.as_str()),
                    },
                    ComparisonOperator::Lt => target_json_column.binary(
                        BinOper::SmallerThan,
                        cast_as_f64(&expression.compare_against)?,
                    ),
                    ComparisonOperator::Lte => target_json_column.binary(
                        BinOper::SmallerThanOrEqual,
                        cast_as_f64(&expression.compare_against)?,
                    ),
                    ComparisonOperator::Gt => target_json_column.binary(
                        BinOper::GreaterThan,
                        cast_as_f64(&expression.compare_against)?,
                    ),
                    ComparisonOperator::Gte => target_json_column.binary(
                        BinOper::GreaterThanOrEqual,
                        cast_as_f64(&expression.compare_against)?,
                    ),
                };
                condition = condition.add(target_json_column);
            }
            // negate this condition if "not" is enabled.
            extra_condition = match search_group.not {
                true => extra_condition.add(condition.not()),
                false => extra_condition.add(condition),
            };
        }
        Ok(extra_condition)
    }

    fn cast_value_array<'a, T>(
        values: &'a Vec<Value>,
        predicate: fn(&'a Value) -> Option<T>,
    ) -> Result<Vec<T>, DatabaseQueryError>
    where
        T: Send,
    {
        values
            .par_iter()
            .map(predicate)
            .collect::<Option<Vec<T>>>()
            .ok_or_else(|| DatabaseQueryError::CastError)
    }
}
