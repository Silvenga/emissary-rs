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
        if self.0.len() >= 12 {
            &self.0[..12]
        } else {
            &self.0
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_id_is_long_enough_then_short_id_should_return_first_12_chars() {
        let id = ContainerId::new("1234567890abcdef1234");

        assert_eq!(id.short_id(), "1234567890ab");
        assert_eq!(id.long_id(), "1234567890abcdef1234");
    }

    #[test]
    fn when_id_is_shorter_than_12_then_short_id_should_return_full_id() {
        let id = ContainerId::new("abc");

        assert_eq!(id.short_id(), "abc");
        assert_eq!(id.long_id(), "abc");
    }

    #[test]
    fn when_id_is_exactly_12_chars_then_short_id_should_equal_long_id() {
        let id = ContainerId::new("1234567890ab");

        assert_eq!(id.short_id(), "1234567890ab");
        assert_eq!(id.long_id(), id.short_id());
    }

    #[test]
    fn when_trying_from_non_empty_string_then_it_should_succeed() {
        let id = ContainerId::try_from("abc123".to_owned()).unwrap();

        assert_eq!(id.long_id(), "abc123");
    }

    #[test]
    fn when_trying_from_empty_string_then_it_should_fail_with_empty_error() {
        let result = ContainerId::try_from(String::new());

        assert!(matches!(result, Err(ContainerIdParseError::Empty)));
    }

    #[test]
    fn when_trying_from_str_then_it_should_behave_like_string_try_from() {
        let id = ContainerId::try_from("abc123").unwrap();

        assert_eq!(id.long_id(), "abc123");
    }

    #[test]
    fn when_trying_from_empty_str_then_it_should_fail() {
        let result = ContainerId::try_from("");

        assert!(matches!(result, Err(ContainerIdParseError::Empty)));
    }

    #[test]
    fn when_converting_to_string_then_it_should_return_full_id() {
        let original = "abc123def456";
        let id = ContainerId::new(original);
        let s: String = id.into();

        assert_eq!(s, original);
    }

    #[test]
    fn when_using_as_ref_then_it_should_return_full_id() {
        let id = ContainerId::new("abc123");

        assert_eq!(id.as_ref(), "abc123");
    }

    #[test]
    fn when_displaying_then_it_should_show_short_id() {
        let id = ContainerId::new("1234567890abcdef");

        assert_eq!(format!("{}", id), "1234567890ab");
    }

    #[test]
    fn when_displaying_short_id_then_it_should_show_full_id() {
        let id = ContainerId::new("abc");

        assert_eq!(format!("{}", id), "abc");
    }
}
