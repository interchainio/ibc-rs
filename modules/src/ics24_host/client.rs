use serde_derive::{Deserialize, Serialize};
use std::str::FromStr;

use super::error::ValidationError;
use super::validate::validate_client_identifier;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClientId(String);

impl ClientId {
    /// Get this identifier as a borrowed `&str`
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get this identifier as a borrowed byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.0.as_bytes()
    }

    /// Get this identifier as an owned `String`
    pub fn to_string(self) -> String {
        self.0
    }
}

impl FromStr for ClientId {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_client_identifier(s).map(|_| Self(s.to_string()))
    }
}
