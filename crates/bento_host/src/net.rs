use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A network endpoint a runtime may connect to or listen on: `host[:port]`, where the host is
/// a hostname, an IPv4 address, or a bracketed IPv6 address (`[::1]:8080`). Without a port,
/// every port of the host is meant. Hosts are kept in one spelling per endpoint (IP addresses
/// in canonical form, hostnames in lowercase), so equal endpoints compare equal; hostnames
/// are never resolved.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NetTarget {
    host: String,
    port: Option<u16>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error(
    "invalid network target `{0}`: expected a hostname, an IPv4 address, or a bracketed IPv6 \
     address, with an optional `:port`"
)]
pub struct InvalidNetTarget(String);

impl NetTarget {
    /// A target for `host`, a hostname or an unbracketed IP address. The host is brought to
    /// its canonical spelling but not validated; parse text that comes from outside the program.
    pub fn new(host: impl Into<String>, port: Option<u16>) -> Self {
        let host = host.into();
        let host = match host.parse::<IpAddr>() {
            Ok(address) => address.to_canonical().to_string(),
            Err(_) => host.to_ascii_lowercase(),
        };
        NetTarget { host, port }
    }

    /// The host in its canonical spelling, without brackets.
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }
}

impl FromStr for NetTarget {
    type Err = InvalidNetTarget;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidNetTarget(text.to_string());
        let (host, port) = match text.strip_prefix('[') {
            Some(rest) => {
                let (address, after) = rest.split_once(']').ok_or_else(invalid)?;
                let address = address.parse::<Ipv6Addr>().map_err(|_| invalid())?;
                let port = match after {
                    "" => None,
                    _ => Some(after.strip_prefix(':').ok_or_else(invalid)?),
                };
                (address.to_string(), port)
            }
            None => {
                let (host, port) = match text.split_once(':') {
                    Some((host, port)) => (host, Some(port)),
                    None => (text, None),
                };
                if host.parse::<Ipv4Addr>().is_err() && !is_hostname(host) {
                    return Err(invalid());
                }
                (host.to_owned(), port)
            }
        };
        let port = port.map(|port| parse_port(port).ok_or_else(invalid)).transpose()?;
        Ok(NetTarget::new(host, port))
    }
}

/// RFC 1123 host name syntax: dot-separated labels of letters, digits, and inner hyphens.
fn is_hostname(host: &str) -> bool {
    host.len() <= 253 && host.split('.').all(is_label) && !ends_in_number(host)
}

fn is_label(label: &str) -> bool {
    (1..=63).contains(&label.len())
        && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        && !label.starts_with('-')
        && !label.ends_with('-')
}

/// Whether the last label is a number, as in `127.1` or `0x7f000001`. Resolvers read such
/// names as IPv4 addresses in legacy notations, so they would give one endpoint several
/// spellings; only the dotted-decimal form is accepted.
fn ends_in_number(host: &str) -> bool {
    let last = host.rsplit('.').next().unwrap_or(host);
    match last.strip_prefix("0x").or_else(|| last.strip_prefix("0X")) {
        Some(hex) => hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
        None => last.bytes().all(|byte| byte.is_ascii_digit()),
    }
}

fn parse_port(port: &str) -> Option<u16> {
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    port.parse().ok()
}

impl fmt::Display for NetTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.host.contains(':'), self.port) {
            (true, Some(port)) => write!(formatter, "[{}]:{port}", self.host),
            (true, None) => write!(formatter, "[{}]", self.host),
            (false, Some(port)) => write!(formatter, "{}:{port}", self.host),
            (false, None) => formatter.write_str(&self.host),
        }
    }
}

impl TryFrom<String> for NetTarget {
    type Error = InvalidNetTarget;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<NetTarget> for String {
    fn from(target: NetTarget) -> Self {
        target.to_string()
    }
}
