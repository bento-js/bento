use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A network endpoint a runtime may connect to or listen on: `host[:port]`, with IPv6
/// hosts bracketed (`[::1]:8080`). Without a port, every port of the host is meant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NetTarget {
    host: String,
    port: Option<u16>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid network target `{0}`: expected host[:port], with IPv6 hosts in brackets")]
pub struct InvalidNetTarget(String);

impl NetTarget {
    pub fn new(host: impl Into<String>, port: Option<u16>) -> Self {
        NetTarget { host: host.into(), port }
    }

    /// The host without brackets.
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
        let (host, port) = if let Some(rest) = text.strip_prefix('[') {
            let (host, after) = rest.split_once(']').ok_or_else(invalid)?;
            match after {
                "" => (host, None),
                _ => (host, Some(after.strip_prefix(':').ok_or_else(invalid)?)),
            }
        } else {
            match text.rsplit_once(':') {
                Some((host, _)) if host.contains(':') => return Err(invalid()),
                Some((host, port)) => (host, Some(port)),
                None => (text, None),
            }
        };
        if host.is_empty() {
            return Err(invalid());
        }
        let port = port.map(|port| port.parse::<u16>().map_err(|_| invalid())).transpose()?;
        Ok(NetTarget::new(host, port))
    }
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
