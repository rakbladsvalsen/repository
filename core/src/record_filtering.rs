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
    sea_query::{extension::postgres::PgBinOper, Alias, BinOper, Expr, Query},
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, ModelTrait, QueryFilter, QuerySelect,
    QueryTrait, RelationTrait, StreamTrait,
};
use sea_query::{IntoCondition, JoinType, SimpleExpr};
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
    JoinColumnEq,
    JoinColumnNotEq,
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum JoinKind {
    Inner,
    Left,
    Right,
    FullOuter,
}

impl JoinKind {
    fn get_db_join_type(&self) -> JoinType {
        match &self {
            Self::Inner => JoinType::InnerJoin,
            Self::Left => JoinType::LeftJoin,
            Self::Right => JoinType::RightJoin,
            Self::FullOuter => JoinType::FullOuterJoin,
        }
    }
}

impl ComparisonOperator {
    /// Whether this operator is a join operator or not.
    fn is_join(&self) -> bool {
        matches!(&self, Self::JoinColumnEq | Self::JoinColumnNotEq)
    }
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
    join_kind: Option<JoinKind>,
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
            // TODO: Properly implement JOIN queries. This will just short-circuit the
            // validation regardless of the type and throw an error.
            ComparisonOperator::JoinColumnEq | ComparisonOperator::JoinColumnNotEq => Err(
                DatabaseQueryError::InvalidUsage("joinEq queries aren't stable".into()),
            ),
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
            // TODO: Properly implement JOIN queries. This will just short-circuit the
            // validation regardless of the type and throw an error.
            ComparisonOperator::JoinColumnEq | ComparisonOperator::JoinColumnNotEq => Err(
                DatabaseQueryError::InvalidUsage("joinEq queries aren't stable".into()),
            ),
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
        info!(
            "ColumnKind: validating {:?} against {:?}",
            self, db_column_kind
        );
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

    pub async fn get_readable_formats_for_user<C: ConnectionTrait + StreamTrait + 'static>(
        self,
        user: &user::Model,
        db: &C,
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
        // Try to fetch the column name and column kind for all formats.
        // Note that there might be more than one format with the same columns,
        // but with different types. In that case, we check if any given column
        // has more than one type (string/number) associated to them.
        let column_and_kind = self
            .formats
            .par_iter()
            .flat_map(|m| &m.schema.0)
            .try_fold(
                HashMap::new,
                |mut hsmap: HashMap<_, HashSet<_>>, col_schema| {
                    hsmap
                        .entry(&col_schema.name)
                        .or_default()
                        .insert(&col_schema.kind);
                    Ok(hsmap)
                },
            )
            .try_reduce(HashMap::new, |mut accum, item| {
                for (column, column_kinds) in item {
                    if column_kinds.len() > 1 {
                        return Err(DatabaseQueryError::ColumnWithMixedTypesError(
                            column.to_string(),
                        ));
                    }
                    let entry = accum.entry(column).or_default();
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
            .map(|argument| {
                // Make sure users don't use join operators with normal comparisons
                if argument.join_kind.is_some() && !argument.comparison_operator.is_join() {
                    return Err(DatabaseQueryError::InvalidUsage(format!(
                        "'{}': cannot use joinKind with this operator ({:?})",
                        argument.column, argument.comparison_operator
                    )));
                }

                match column_and_kind.get(&argument.column) {
                    Some(column_kind) => argument.validate(column_kind),
                    _ => Err(DatabaseQueryError::InvalidColumnRequested(
                        argument.column.to_string(),
                    )),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Validate ColumnEq searches.
        // When using ColumnEq, we need to check for two conditions:
        // - The target column that is being compared against must exist
        // - The source column type and target column type must have
        //   the same data type, i.e. we can only compare string columns against
        //   string columns and so on.
        self.query
            .query
            .par_iter()
            .flat_map(|group| &group.args)
            .filter(|it| it.comparison_operator.is_join())
            .map(|it| {
                if it.join_kind.is_none() {
                    return Err(DatabaseQueryError::InvalidUsage(format!(
                        "'{}': operator '{:?}' needs a joinKind",
                        it.column, it.comparison_operator
                    )));
                }
                let source_column_type = column_and_kind.get(&it.column);
                let compare_column = it
                    .compare_against
                    .as_str()
                    .ok_or(DatabaseQueryError::InvalidUsage(
                        "compareAgainst must be a valid column".into(),
                    ))?
                    .to_string();
                let compare_column_type = column_and_kind.get(&compare_column);
                let column_types_matched = source_column_type
                    .zip(compare_column_type)
                    .map(|(a, b)| a == b)
                    .unwrap_or(false);
                if !column_types_matched {
                    return Err(DatabaseQueryError::InvalidColumnRequested(compare_column));
                }
                Ok(())
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
        Condition::all().add(record::Column::FormatId.is_in(self.get_readable_format_ids()))
    }

    fn apply_query_parameter_filters(&self) -> Option<Condition> {
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

    pub fn build_condition_for_arg(
        &self,
        column_kind: &ColumnKind,
        expression: &SearchArguments,
    ) -> Result<SimpleExpr, DatabaseQueryError> {
        let mut target_json_column = Expr::col(record::Column::Data)
            .binary(PgBinOper::CastJsonField, Expr::val(&expression.column));

        // determine whether this filter needs cast to FLOAT (postgres needs it)
        let cast_as_f64 = |value: &serde_json::Value| -> Result<f64, DatabaseQueryError> {
            value.as_f64().ok_or_else(|| DatabaseQueryError::CastError)
        };
        if (*column_kind).eq(&ColumnKind::Number) {
            target_json_column = target_json_column.cast_as(Alias::new("FLOAT"));
        }

        target_json_column = match expression.comparison_operator {
            // Array comparison
            ComparisonOperator::In => {
                let array = expression
                    .compare_against
                    .as_array()
                    .expect("arg isn't array");
                match column_kind {
                    ColumnKind::Number => {
                        let casted = PreparedSearchQuery::cast_value_array(array, |v| v.as_f64())?;
                        Expr::expr(target_json_column).is_in(casted)
                    }
                    ColumnKind::String => {
                        let casted = PreparedSearchQuery::cast_value_array(array, |v| v.as_str())?;
                        Expr::expr(target_json_column).is_in(casted)
                    }
                }
            }
            ComparisonOperator::Regex => {
                target_json_column.binary(PgBinOper::Regex, expression.compare_against.as_str())
            }
            ComparisonOperator::RegexCaseInsensitive => target_json_column.binary(
                PgBinOper::RegexCaseInsensitive,
                expression.compare_against.as_str(),
            ),
            ComparisonOperator::ILike => {
                target_json_column.binary(PgBinOper::ILike, expression.compare_against.as_str())
            }
            ComparisonOperator::Like => {
                target_json_column.binary(BinOper::Like, expression.compare_against.as_str())
            }
            ComparisonOperator::Eq => {
                match column_kind {
                    ColumnKind::Number => target_json_column
                        .binary(BinOper::Equal, expression.compare_against.as_f64()),
                    ColumnKind::String => target_json_column
                        .binary(BinOper::Equal, expression.compare_against.as_str()),
                }
            }
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
            _ => {
                unimplemented!()
            }
        };
        Ok(target_json_column)
    }

    fn apply_join_filter<E>(
        &self,
        column_kind: &ColumnKind,
        join_kind: JoinKind,
        expression: &SearchArguments,
        select: sea_orm::Select<E>,
    ) -> sea_orm::Select<E>
    where
        E: sea_orm::EntityTrait,
    {
        let random_alias = "asdadsasd";
        let left_column = expression.column.clone();
        let right_column = expression.compare_against.clone();
        let column_kind = column_kind.to_owned();
        let operator = expression.comparison_operator;

        let relation = record::Relation::Data
            .def()
            .on_condition(move |left, right| {
                debug!("right column: [{}]", right_column);
                let mut left = Expr::col(left)
                    .binary(BinOper::Custom("."), Expr::col(record::Column::Data))
                    .binary(PgBinOper::CastJsonField, left_column.as_str());
                let mut right = Expr::col(right)
                    .binary(BinOper::Custom("."), Expr::col(record::Column::Data))
                    .binary(PgBinOper::CastJsonField, Expr::val(right_column.as_str()));
                if column_kind.eq(&ColumnKind::Number) {
                    left = left.cast_as(Alias::new("FLOAT"));
                    right = right.cast_as(Alias::new("FLOAT"));
                }
                match operator {
                    ComparisonOperator::JoinColumnEq => right.eq(left).into_condition(),
                    ComparisonOperator::JoinColumnNotEq => right.ne(left).into_condition(),
                    _ => todo!(),
                }
            });

        select.join_as(
            join_kind.get_db_join_type(),
            relation,
            Alias::new(random_alias),
        )
    }

    pub fn apply_condition<E>(
        self,
        mut select: sea_orm::Select<E>,
    ) -> Result<sea_orm::Select<E>, DatabaseQueryError>
    where
        E: sea_orm::EntityTrait,
    {
        let start = std::time::Instant::now();
        let requested_search_columns = self.get_columns_and_verify_types()?;
        let end = std::time::Instant::now() - start;
        info!("took: {:?} to validate types", end);

        // create extra filtering condition to search inside ALL JSONB hashmaps
        let mut condition = Condition::all();
        condition = condition.add(self.limit_visible_records());
        // apply upload session filters, if any was passed.
        if let Some(c) = self.apply_query_parameter_filters() {
            condition = condition.add(c);
        }

        for search_group in self.query.query.iter() {
            // iterate over all search groups and create conditions for each one
            let mut group_condition = search_group.get_condition_type();
            // iterate over all expressions inside this group
            for expression in search_group.args.iter() {
                let column_kind = requested_search_columns
                    .get(&expression.column)
                    .ok_or_else(|| {
                        // This should never happen as we previously validated the column type.
                        error!(
                            "INVALID: couldn't find column kind for column {}",
                            expression.column
                        );
                        DatabaseQueryError::InvalidColumnRequested(expression.column.clone())
                    })?;

                if let Some(join_kind) = expression.join_kind {
                    select = self.apply_join_filter(column_kind, join_kind, expression, select)
                } else {
                    group_condition =
                        group_condition.add(self.build_condition_for_arg(column_kind, expression)?);
                }
            }

            // only add this condition if something was actually added, i.e.
            // if the argument was a non-join statement.
            if !group_condition.is_empty() {
                // negate this entire search group if "not" is enabled.
                condition = match search_group.not {
                    true => condition.add(group_condition.not()),
                    false => condition.add(group_condition),
                };
            }
        }
        Ok(select.filter(condition))
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
