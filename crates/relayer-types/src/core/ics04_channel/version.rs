//! Data type definition and utilities for the
//! version field of a channel end.
//!

use serde_derive::{Deserialize, Serialize};
use serde_json as json;
use std::convert::Infallible;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::str::FromStr;
use tracing::debug;

use crate::applications::transfer;

/// The version field for a `ChannelEnd`.
///
/// This field is opaque to the core IBC protocol.
/// No explicit validation is necessary, and the
/// spec (v1) currently allows empty strings.
#[derive(Clone, Debug, Eq, Deserialize, Serialize)]
pub struct Version(pub String);

impl Version {
    pub fn new(v: String) -> Self {
        Self(v)
    }

    pub fn ics20(version: u64) -> Self {
        Self::new(format!("{}-{version}", transfer::VERSION_PREFIX))
    }

    pub fn ics20_with_fee(version: u64) -> Self {
        let val = json::json!({
            "fee_version": "ics29-1",
            "app_version": format!("{}-{version}", transfer::VERSION_PREFIX),
        });

        Self::new(val.to_string())
    }

    pub fn app_version_with_fee(app_version: &str) -> Self {
        let val = json::json!({
            "fee_version": "ics29-1",
            "app_version": app_version,
        });

        Self::new(val.to_string())
    }

    pub fn empty() -> Self {
        Self::new("".to_string())
    }

    pub fn supports_fee(&self) -> bool {
        json::from_str::<json::Value>(&self.0)
            .ok()
            .and_then(|val| {
                let _app_version = val.get("app_version")?.as_str()?;

                let fee_version = val.get("fee_version")?.as_str()?;

                Some(fee_version == "ics29-1")
            })
            .unwrap_or(false)
    }

    pub fn is_ics20_v2(&self) -> bool {
        match serde_json::from_str::<serde_json::Value>(&self.0) {
            Ok(json_value) => {
                let app_version = json_value
                    .get("app_version")
                    .and_then(|app_version| app_version.as_str())
                    .unwrap_or(self.0.as_str());
                app_version == "ics20-2"
            }
            Err(e) => {
                debug!("failed to deserialise version as JSON will fallback to direct string comparison. Error: {e}");
                self.0.as_str() == "ics20-2"
            }
        }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        if self.0 != other.0 {
            // If the Version strings don't match, check that this isn't due to the json
            // fields being in a different order
            let parsed_version = match serde_json::from_str::<json::Value>(&self.0) {
                Ok(value) => value,
                Err(_) => return false,
            };
            let parsed_other = match serde_json::from_str::<json::Value>(&other.to_string()) {
                Ok(value) => value,
                Err(_) => return false,
            };

            if parsed_version != parsed_other {
                return false;
            }
        }
        true
    }
}

impl From<String> for Version {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl FromStr for Version {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.to_string()))
    }
}

/// The default version is empty (unspecified).
impl Default for Version {
    fn default() -> Self {
        Version::empty()
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod test {
    use super::Version;

    #[test]
    fn test_ics29_version() {
        {
            let version = Version::ics20(1);
            assert!(!version.supports_fee());
        }

        {
            let version = Version::ics20_with_fee(1);
            assert!(version.supports_fee());
        }
    }
}
