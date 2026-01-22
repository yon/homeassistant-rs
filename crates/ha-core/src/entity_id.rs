//! Entity ID type representing a domain.object_id pair

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Error type for invalid entity IDs
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum EntityIdError {
    #[error("entity_id must contain exactly one '.' separator")]
    InvalidFormat,

    #[error("domain cannot be empty")]
    EmptyDomain,

    #[error("object_id cannot be empty")]
    EmptyObjectId,

    #[error(
        "domain contains invalid characters (must be lowercase alphanumeric with underscores, cannot start/end with underscore or contain double underscores)"
    )]
    InvalidDomainChars,

    #[error(
        "object_id contains invalid characters (must be lowercase alphanumeric with underscores, cannot start/end with underscore)"
    )]
    InvalidObjectIdChars,
}

/// Represents a Home Assistant entity ID (e.g., "light.living_room")
///
/// Entity IDs consist of a domain and an object_id separated by a period.
/// Both parts must be lowercase alphanumeric with underscores only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntityId {
    domain: String,
    object_id: String,
}

impl EntityId {
    /// Create a new EntityId from domain and object_id parts
    pub fn new(
        domain: impl Into<String>,
        object_id: impl Into<String>,
    ) -> Result<Self, EntityIdError> {
        let domain = domain.into();
        let object_id = object_id.into();

        if domain.is_empty() {
            return Err(EntityIdError::EmptyDomain);
        }
        if object_id.is_empty() {
            return Err(EntityIdError::EmptyObjectId);
        }
        if !Self::is_valid_domain(&domain) {
            return Err(EntityIdError::InvalidDomainChars);
        }
        if !Self::is_valid_object_id(&object_id) {
            return Err(EntityIdError::InvalidObjectIdChars);
        }

        Ok(Self { domain, object_id })
    }

    /// Get the domain part of the entity ID
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Get the object_id part of the entity ID
    pub fn object_id(&self) -> &str {
        &self.object_id
    }

    /// Check if an object_id is valid (lowercase alphanumeric + underscore, cannot start/end with _)
    ///
    /// Matches Python HA regex: `(?!_)[\da-z_]+(?<!_)`
    fn is_valid_object_id(s: &str) -> bool {
        // Cannot start or end with underscore
        if s.starts_with('_') || s.ends_with('_') {
            return false;
        }
        // Must contain only lowercase alphanumeric and underscores
        s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// Check if a domain is valid (same as object_id, plus cannot contain __)
    ///
    /// Matches Python HA regex: `(?!.+__)(?!_)[\da-z_]+(?<!_)`
    fn is_valid_domain(s: &str) -> bool {
        // Domain cannot contain double underscores
        if s.contains("__") {
            return false;
        }
        // Otherwise same rules as object_id
        Self::is_valid_object_id(s)
    }
}

impl FromStr for EntityId {
    type Err = EntityIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 2 {
            return Err(EntityIdError::InvalidFormat);
        }
        Self::new(parts[0], parts[1])
    }
}

impl TryFrom<String> for EntityId {
    type Error = EntityIdError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<EntityId> for String {
    fn from(id: EntityId) -> String {
        id.to_string()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.domain, self.object_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_entity_id() {
        let id = EntityId::new("light", "living_room").unwrap();
        assert_eq!(id.domain(), "light");
        assert_eq!(id.object_id(), "living_room");
        assert_eq!(id.to_string(), "light.living_room");
    }

    #[test]
    fn test_parse_entity_id() {
        let id: EntityId = "sensor.temperature".parse().unwrap();
        assert_eq!(id.domain(), "sensor");
        assert_eq!(id.object_id(), "temperature");
    }

    #[test]
    fn test_invalid_format() {
        assert_eq!(
            "no_separator".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidFormat
        );
        assert_eq!(
            "too.many.parts".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidFormat
        );
    }

    #[test]
    fn test_empty_parts() {
        assert_eq!(
            ".object".parse::<EntityId>().unwrap_err(),
            EntityIdError::EmptyDomain
        );
        assert_eq!(
            "domain.".parse::<EntityId>().unwrap_err(),
            EntityIdError::EmptyObjectId
        );
    }

    #[test]
    fn test_invalid_chars() {
        assert_eq!(
            "UPPER.case".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidDomainChars
        );
        assert_eq!(
            "light.UPPER".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidObjectIdChars
        );
        assert_eq!(
            "with-dash.object".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidDomainChars
        );
    }

    #[test]
    fn test_underscore_rules() {
        // Leading underscore in domain - invalid
        assert_eq!(
            "_light.room".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidDomainChars
        );
        // Trailing underscore in domain - invalid
        assert_eq!(
            "light_.room".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidDomainChars
        );
        // Leading underscore in object_id - invalid
        assert_eq!(
            "light._room".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidObjectIdChars
        );
        // Trailing underscore in object_id - invalid
        assert_eq!(
            "light.room_".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidObjectIdChars
        );
        // Double underscore in domain - invalid
        assert_eq!(
            "my__light.room".parse::<EntityId>().unwrap_err(),
            EntityIdError::InvalidDomainChars
        );
        // Double underscore in object_id - valid (Python allows this)
        assert!("light.my__room".parse::<EntityId>().is_ok());
        // Middle underscores are fine
        assert!("my_light.living_room".parse::<EntityId>().is_ok());
    }

    #[test]
    fn test_serde_roundtrip() {
        let id = EntityId::new("switch", "kitchen").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"switch.kitchen\"");

        let parsed: EntityId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }
}
