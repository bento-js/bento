use std::collections::BTreeMap;
use std::env;

use crate::error::CliError;

/// The environment `bento` was started with.
pub(crate) fn read() -> Result<BTreeMap<String, String>, CliError> {
    env::vars_os()
        .map(|(name, value)| match (name.into_string(), value.into_string()) {
            (Ok(name), Ok(value)) => Ok((name, value)),
            (name, _) => {
                let name = name.unwrap_or_else(|name| name.to_string_lossy().into_owned());
                Err(CliError::NonUnicodeVariable(name))
            }
        })
        .collect()
}
