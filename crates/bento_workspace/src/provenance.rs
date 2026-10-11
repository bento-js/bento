use std::collections::BTreeMap;

use thiserror::Error;

/// Environment variable through which a launch tells nested Bento launches which of
/// the variables it hands them were loaded from environment files.
///
/// Each entry is `NAME:fingerprint`, comma-separated. A launch leaves out the variables
/// the task command assigns itself. The fingerprint identifies the loaded value, so a
/// variable changed any other way also counts as an explicit value again.
pub const FILE_KEYS_ENV: &str = "BENTO_ENV_FILE_KEYS";

#[derive(Debug, Error)]
#[error("{FILE_KEYS_ENV} is malformed; expected comma-separated `NAME:fingerprint` entries")]
pub struct InvalidFileKeys;

/// An inherited environment divided by where its values came from.
pub(crate) struct Inherited {
    /// Values the user or the invoking command set.
    pub actual: BTreeMap<String, String>,
    /// Values an outer launch loaded from its environment files.
    pub forwarded: BTreeMap<String, String>,
}

pub(crate) fn split(mut inherited: BTreeMap<String, String>) -> Result<Inherited, InvalidFileKeys> {
    let recorded = match inherited.remove(FILE_KEYS_ENV) {
        Some(entries) => decode(&entries)?,
        None => BTreeMap::new(),
    };
    let (forwarded, actual) = inherited
        .into_iter()
        .partition(|(name, value)| recorded.get(name) == Some(&fingerprint(value)));
    Ok(Inherited { actual, forwarded })
}

pub(crate) fn encode<'a>(
    file_keys: impl IntoIterator<Item = &'a String>,
    vars: &BTreeMap<String, String>,
) -> String {
    let entries: Vec<String> = file_keys
        .into_iter()
        .filter_map(|name| {
            vars.get(name).map(|value| format!("{name}:{:016x}", fingerprint(value)))
        })
        .collect();
    entries.join(",")
}

fn decode(entries: &str) -> Result<BTreeMap<String, u64>, InvalidFileKeys> {
    entries
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (name, hex) = entry.rsplit_once(':').ok_or(InvalidFileKeys)?;
            let fingerprint = u64::from_str_radix(hex, 16).map_err(|_| InvalidFileKeys)?;
            Ok((name.to_owned(), fingerprint))
        })
        .collect()
}

/// 64-bit FNV-1a. Stable across builds, unlike `std`'s hasher, because the nested
/// `bento` reading it need not be the same binary that wrote it.
fn fingerprint(value: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    value.bytes().fold(OFFSET_BASIS, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(PRIME))
}
