use std::collections::{HashMap, HashSet};

use actix_example_core::{format::ColumnKind, record::DynamicHashmap};
use entity::format::Model as FormatModel;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

use crate::error::{APIError, ValidationFailureKind};

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
            .0
            .iter()
            .map(|format| format.name.to_owned())
            .collect::<HashSet<_>>();

        let schema: HashMap<_, _> = inbound
            .schema
            .0
            .iter()
            .map(|f| (f.name.clone(), f.kind.clone()))
            .collect::<HashMap<_, _>>();

        let is_error = self.data.par_iter().find_map_any(|hmap| {
            // Validate ALL dicts have the keys present in the schema, otherwise
            // error out
            if hmap
                .keys()
                .map(|v| v.to_owned())
                .collect::<HashSet<_>>()
                != valid_keys
            {
                return Some(APIError::ValidationFailure(
                    ValidationFailureKind::MissingDictKeys,
                ));
            }
            // Validate whether the values in each map have the right data
            // type, i.e. a String actually has a string and not something else
            if hmap.iter().any(|(key, value)| {
                // note: this `key` is guaranteed to exist in `schema` since
                // we already validated it
                match schema.get(key).unwrap() {
                    ColumnKind::Number => value.as_f64().is_none(),
                    ColumnKind::String => value.as_str().is_none(),
                }
            }) {
                return Some(APIError::ValidationFailure(
                    ValidationFailureKind::MismatchedDataType,
                ));
            }

            // This dict passed the two validations above, keep iterating
            None
        });

        match is_error {
            Some(err_msg) => Err(err_msg),
            _ => Ok(()),
        }
    }
}
