use serde::{Deserialize, Serialize};
use std::fmt::Display;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContainerId(String);

impl ContainerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn short_id(&self) -> &str {
        &self.0[..12]
    }

    pub fn long_id(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ContainerId {
    type Error = ContainerIdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(Self::Error::Empty);
        }
        Ok(Self(value))
    }
}

impl TryFrom<&str> for ContainerId {
    type Error = ContainerIdParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.to_owned().try_into()
    }
}

impl AsRef<str> for ContainerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ContainerId> for String {
    fn from(value: ContainerId) -> Self {
        value.0
    }
}

impl Display for ContainerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.short_id())
    }
}

#[derive(Error, Debug)]
pub enum ContainerIdParseError {
    #[error("Invalid container ID format, id was empty")]
    Empty,
}
