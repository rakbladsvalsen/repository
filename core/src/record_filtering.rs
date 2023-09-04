use std::collections::{HashMap, HashSet};

use entity::{
    error::DatabaseQueryError,
    format::{self, ColumnKind},
    format_entitlement::{self, Access},
    record,
    traits::AsQueryParamFilterable,
    upload_session, user,
};
use log::{debug, error, info};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use sea_orm::{
    sea_query::{extension::postgres::PgBinOper, BinOper, Expr, Query},
    ColumnTrait, Condition, DbConn, EntityTrait, ModelTrait, QueryFilter, QuerySelect, QueryTrait,
};
use serde::*;
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
/// Proxy for sea_query's supported condition types.
pub enum ConditionKind {
    Any,
    All,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
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

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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
                _ => Err(DatabaseQueryError::InvalidUsage(format!(
                    "cannot use operator '{:?}' on non-string types",
                    self.comparison_operator
                ))),
            },
            _ => Err(DatabaseQueryError::InvalidUsage(
                "cannot use numeric operator on string columns".into(),
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
        info!("validating column kind: {:?}", db_column_kind);
        match db_column_kind {
            ColumnKind::Number => self.validate_number(),
            ColumnKind::String => self.validate_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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

impl SearchGroup {
    fn get_condition_type(&self) -> Condition {
        match self.condition_kind {
            ConditionKind::All => Condition::all(),
            ConditionKind::Any => Condition::any(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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

#[derive(Debug)]
pub struct PreparedSearchQuery {
    formats: Vec<format::Model>,
    query: SearchQuery,
}

impl SearchQuery {
    pub fn validate(&self) -> Result<(), DatabaseQueryError> {
        // validate the query vec isn't empty.
        // if the list of formats is defined, ensure it isn't empty.
        match self.formats.as_ref() {
            Some(it) => {
                if it.is_empty() {
                    Err(DatabaseQueryError::EmptyQuery)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    pub async fn get_readable_formats_for_user(
        self,
        user: &user::Model,
        db: &DbConn,
    ) -> Result<PreparedSearchQuery, DatabaseQueryError> {
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

        Ok(PreparedSearchQuery {
            formats: filtered_formats.all(db).await?,
            query: self,
        })
    }
}

impl PreparedSearchQuery {
    /// Get all the available columns in the schema. This is useful if we're building a
    /// csv file, since we have to know in beforehand the available columns that we might have.
    pub fn schema_columns(&self) -> HashSet<String> {
        self.formats
            .par_iter()
            .flat_map(|fmt| &fmt.schema.0)
            .map(|schema| schema.name.clone())
            .collect()
    }

    /// Perform basic checks.
    fn get_columns_and_verify_types(
        &self,
    ) -> Result<HashMap<&String, &ColumnKind>, DatabaseQueryError> {
        let column_and_kind = self
            .formats
            .par_iter()
            .flat_map(|m| &m.schema.0)
            .try_fold(HashMap::new, |mut hsmap, col_schema| {
                hsmap
                    .entry(&col_schema.name)
                    .or_insert_with(HashSet::new)
                    .insert(&col_schema.kind);
                Ok(hsmap)
            })
            .try_reduce(HashMap::new, |mut accum, item| {
                for (column, column_kinds) in item {
                    if column_kinds.len() > 1 {
                        return Err(DatabaseQueryError::ColumnWithMixedTypesError(
                            column.to_string(),
                        ));
                    }
                    let entry = accum.entry(column).or_insert_with(HashSet::new);
                    entry.extend(column_kinds);
                    if entry.len() > 1 {
                        return Err(DatabaseQueryError::ColumnWithMixedTypesError(
                            column.to_string(),
                        ));
                    }
                }
                Ok(accum)
            })
            .map(|it| {
                // Convert HashMap<&String, HashSet<&ColumnKind>> to HashMap<&String, &ColumnKind>
                // Note that we're sure there'll be a single ColumnKind
                it.into_par_iter()
                    .map(|(k, v)| (k, v.into_iter().next().expect("missing ColumnKind")))
                    .collect::<HashMap<_, _>>()
            })?;

        // We already have the right column types, so let's just use them to validate
        // user-defined columns. We can also validate if the user passed a non-existent column,
        // in one go. Yay!
        self.query
            .query
            .par_iter()
            .flat_map(|group| &group.args)
            .map(|argument| match column_and_kind.get(&argument.column) {
                Some(column_kind) => argument.validate(column_kind),
                _ => Err(DatabaseQueryError::InvalidColumnRequested(
                    argument.column.to_string(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(column_and_kind)
    }

    /// Build a vec with the IDs of readable formats.
    #[inline(always)]
    fn get_readable_format_ids(&self) -> Vec<i32> {
        self.formats.par_iter().map(|model| model.id).collect()
    }

    /// Limit the available visible records.
    #[inline(always)]
    fn limit_visible_records(&self) -> Condition {
        let subquery = Query::select()
            .column(upload_session::Column::Id)
            .cond_where(upload_session::Column::FormatId.is_in(self.get_readable_format_ids()))
            .from(upload_session::Entity)
            .to_owned();
        Condition::all().add(record::Column::UploadSessionId.in_subquery(subquery))
    }

    fn apply_upload_session_filters(&self) -> Option<Condition> {
        if let Some(upload_session_filters) = self.query.upload_session.as_ref() {
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
            return Some(
                Condition::all().add(record::Column::UploadSessionId.in_subquery(condition)),
            );
        }
        None
    }

    pub fn build_condition(self) -> Result<Condition, DatabaseQueryError> {
        let start = std::time::Instant::now();
        let requested_search_columns = self.get_columns_and_verify_types()?;
        let end = std::time::Instant::now() - start;
        info!("took: {:?} to validate types", end);

        // create extra filtering condition to search inside ALL JSONB hashmaps
        let mut condition = Condition::all();
        condition = condition.add(self.limit_visible_records());
        // apply upload session filters, if any was passed.
        if let Some(c) = self.apply_upload_session_filters() {
            condition = condition.add(c);
        }

        for search_group in self.query.query.iter() {
            // iterate over all search groups and create conditions for each one
            let mut group_condition = search_group.get_condition_type();
            // iterate over all expressions inside this group
            for expression in search_group.args.iter() {
                let mut target_json_column = Expr::col(record::Column::Data)
                    .binary(PgBinOper::CastJsonField, Expr::val(&expression.column));

                // Get the database type for this column.
                let against_column_kind = requested_search_columns
                    .get(&expression.column)
                    .ok_or_else(|| {
                        // This should never happen as we previously validated the column type.
                        error!(
                            "INVALID: couldn't find column kind for column {}",
                            expression.column
                        );
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
                        let array = expression
                            .compare_against
                            .as_array()
                            .expect("arg isn't array");
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
                    ComparisonOperator::Regex => target_json_column
                        .binary(PgBinOper::Regex, expression.compare_against.as_str()),
                    ComparisonOperator::RegexCaseInsensitive => target_json_column.binary(
                        PgBinOper::RegexCaseInsensitive,
                        expression.compare_against.as_str(),
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
                group_condition = group_condition.add(target_json_column);
            }
            // negate this condition if "not" is enabled.
            condition = match search_group.not {
                true => condition.add(group_condition.not()),
                false => condition.add(group_condition),
            };
        }
        Ok(condition)
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
