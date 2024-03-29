use std::collections::{HashMap, HashSet};

use central_repository_dao::{format::ColumnKind, record::DynamicHashmap, str_to_isodate};
use entity::format::Model as FormatModel;
use itertools::Itertools;
use log::{debug, error, info};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    common::handle_fatal,
    error::{APIError, ValidationFailureKind},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InboundRecordData {
    pub format_id: i32,
    pub data: Vec<DynamicHashmap>,
}

impl InboundRecordData {
    pub fn validate_blocking(&self, inbound: &FormatModel) -> Result<(), APIError> {
        let valid_keys = inbound
            .schema
            .iter()
            .map(|format| &format.name)
            .sorted()
            .collect::<HashSet<_>>();
        debug!("valid hashmap keys are: {:?}", valid_keys);

        let schema = inbound
            .schema
            .iter()
            .map(|f| (&f.name, &f.kind))
            .collect::<HashMap<_, _>>();

        let column_to_regex = inbound
            .schema
            .iter()
            .flat_map(|schema| schema.regex.as_ref().map(|regex| (&schema.name, regex)))
            .map(|(name, regex)| {
                Ok((
                    name,
                    // note: this regex is guaranteed to be valid since we validated it when
                    // we created the model
                    Regex::new(regex.as_str()).map_err(|err| {
                        handle_fatal!("regex is invalid", err, APIError::ServerError)
                    })?,
                ))
            })
            .collect::<Result<HashMap<_, _>, APIError>>()?;

        debug!("column to regex mapping: {:#?}", column_to_regex);

        let is_error = self.data.par_iter().find_map_any(|hmap| {
            if hmap.keys().len() != valid_keys.len() {
                info!(
                    "hmap key length error: input has {} keys, but expected {}",
                    hmap.keys().len(),
                    valid_keys.len()
                );
                return Some(APIError::ValidationFailure(
                    ValidationFailureKind::MissingDictKeys,
                ));
            }

            let hmap_keys_sorted = hmap.keys().sorted().collect::<HashSet<&String>>();
            // Validate ALL dicts have the keys present in the schema, otherwise
            // error out
            if !valid_keys.eq(&hmap_keys_sorted) {
                info!(
                    "hmap key mismatch: got {:?}, expected {:?}",
                    hmap_keys_sorted, valid_keys
                );
                return Some(APIError::ValidationFailure(
                    ValidationFailureKind::MissingDictKeys,
                ));
            }
            // Validate whether the values in each map have the right data type
            if hmap.iter().any(|(key, value)| {
                if let Some(column_kind) = schema.get(key) {
                    match *column_kind {
                        ColumnKind::Number => value.as_f64().is_none(),
                        ColumnKind::String => value.as_str().is_none(),
                        ColumnKind::Datetime => value
                            .as_str()
                            .map(|s| str_to_isodate(s).is_none())
                            .unwrap_or(true),
                    }
                } else {
                    true
                }
            }) {
                return Some(APIError::ValidationFailure(
                    ValidationFailureKind::MismatchedDataType,
                ));
            }

            // match regex, if enabled
            if column_to_regex.iter().any(|(key, regex)| {
                if let Some(value) = hmap.get(*key).and_then(|v| v.as_str()) {
                    !regex.is_match(value)
                } else {
                    true
                }
            }) {
                info!("regex match failure for map: {:#?}", hmap);
                return Some(APIError::ValidationFailure(
                    ValidationFailureKind::RegexMatchFailure,
                ));
            }

            // This dict passed the two validations above, keep iterating
            None
        });

        is_error.map_or_else(|| Ok(()), Err)
    }
}
