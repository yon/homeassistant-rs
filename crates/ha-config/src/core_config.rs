//! Core Home Assistant configuration
//!
//! Parses the `homeassistant:` section from configuration.yaml

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::path::Path;

use crate::error::{ConfigError, ConfigResult};
use crate::loader::load_yaml;

/// Unit system configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnitSystem {
    pub length: String,
    pub accumulated_precipitation: String,
    pub mass: String,
    pub pressure: String,
    pub temperature: String,
    pub volume: String,
    pub wind_speed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
}

impl UnitSystem {
    /// Create metric unit system
    pub fn metric() -> Self {
        Self {
            length: "km".to_string(),
            accumulated_precipitation: "mm".to_string(),
            mass: "g".to_string(),
            pressure: "Pa".to_string(),
            temperature: "°C".to_string(),
            volume: "L".to_string(),
            wind_speed: "m/s".to_string(),
            area: Some("m²".to_string()),
        }
    }

    /// Create imperial unit system
    pub fn imperial() -> Self {
        Self {
            length: "mi".to_string(),
            accumulated_precipitation: "in".to_string(),
            mass: "lb".to_string(),
            pressure: "psi".to_string(),
            temperature: "°F".to_string(),
            volume: "gal".to_string(),
            wind_speed: "mph".to_string(),
            area: Some("ft²".to_string()),
        }
    }
}

/// Core Home Assistant configuration from the `homeassistant:` section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    /// Name of the location (e.g., "Home")
    #[serde(default = "default_name")]
    pub name: String,

    /// Latitude of the location
    #[serde(default)]
    pub latitude: f64,

    /// Longitude of the location
    #[serde(default)]
    pub longitude: f64,

    /// Elevation in meters
    #[serde(default)]
    pub elevation: i32,

    /// Unit system (metric or imperial)
    #[serde(default)]
    pub unit_system: UnitSystemConfig,

    /// Time zone (e.g., "America/Los_Angeles")
    #[serde(default = "default_time_zone")]
    pub time_zone: String,

    /// Currency code (e.g., "USD")
    #[serde(default = "default_currency")]
    pub currency: String,

    /// Country code (e.g., "US")
    #[serde(default)]
    pub country: Option<String>,

    /// Language code (e.g., "en")
    #[serde(default = "default_language")]
    pub language: String,

    /// Radius around home zone in meters
    #[serde(default = "default_radius")]
    pub radius: i32,

    /// External URL for accessing HA
    #[serde(default)]
    pub external_url: Option<String>,

    /// Internal URL for accessing HA
    #[serde(default)]
    pub internal_url: Option<String>,

    /// Allowlist of external directories
    #[serde(default)]
    pub allowlist_external_dirs: Vec<String>,

    /// Allowlist of external URLs
    #[serde(default)]
    pub allowlist_external_urls: Vec<String>,

    /// Auth providers configuration
    #[serde(default)]
    pub auth_providers: Vec<Value>,
}

/// Unit system configuration - can be "metric", "imperial", or custom
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum UnitSystemConfig {
    /// Named unit system
    #[default]
    Metric,
    /// Custom unit system name
    Named(String),
    /// Full custom unit system
    Custom(UnitSystem),
}

impl UnitSystemConfig {
    /// Convert to the full UnitSystem struct
    pub fn to_unit_system(&self) -> UnitSystem {
        match self {
            UnitSystemConfig::Metric => UnitSystem::metric(),
            UnitSystemConfig::Named(name) if name == "imperial" => UnitSystem::imperial(),
            UnitSystemConfig::Named(_) => UnitSystem::metric(),
            UnitSystemConfig::Custom(custom) => custom.clone(),
        }
    }
}

fn default_name() -> String {
    "Home".to_string()
}

fn default_time_zone() -> String {
    "UTC".to_string()
}

fn default_currency() -> String {
    "USD".to_string()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_radius() -> i32 {
    100
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            name: default_name(),
            latitude: 0.0,
            longitude: 0.0,
            elevation: 0,
            unit_system: UnitSystemConfig::Metric,
            time_zone: default_time_zone(),
            currency: default_currency(),
            country: None,
            language: default_language(),
            radius: default_radius(),
            external_url: None,
            internal_url: None,
            allowlist_external_dirs: Vec::new(),
            allowlist_external_urls: Vec::new(),
            auth_providers: Vec::new(),
        }
    }
}

impl CoreConfig {
    /// Load core configuration from a config directory
    pub fn load(config_dir: impl AsRef<Path>) -> ConfigResult<Self> {
        let config_dir = config_dir.as_ref();
        let yaml = load_yaml(config_dir, "configuration.yaml")?;

        Self::from_yaml(&yaml)
    }

    /// Parse core configuration from YAML value
    pub fn from_yaml(yaml: &Value) -> ConfigResult<Self> {
        let mapping = yaml.as_mapping().ok_or_else(|| ConfigError::InvalidValue {
            key: "root".to_string(),
            reason: "configuration must be a mapping".to_string(),
        })?;

        // Get the homeassistant section, or use defaults
        let ha_section = mapping
            .get(&Value::String("homeassistant".to_string()))
            .cloned()
            .unwrap_or(Value::Mapping(serde_yaml::Mapping::new()));

        // Parse the homeassistant section
        let config: CoreConfig = serde_yaml::from_value(ha_section).map_err(|e| {
            ConfigError::InvalidValue {
                key: "homeassistant".to_string(),
                reason: e.to_string(),
            }
        })?;

        Ok(config)
    }

    /// Get the resolved unit system
    pub fn unit_system(&self) -> UnitSystem {
        self.unit_system.to_unit_system()
    }

    /// Convert to the API response format for /api/config
    pub fn to_api_response(&self, version: &str, components: &[String]) -> serde_json::Value {
        let unit_system = self.unit_system();

        serde_json::json!({
            "latitude": self.latitude,
            "longitude": self.longitude,
            "elevation": self.elevation,
            "unit_system": {
                "length": unit_system.length,
                "accumulated_precipitation": unit_system.accumulated_precipitation,
                "mass": unit_system.mass,
                "pressure": unit_system.pressure,
                "temperature": unit_system.temperature,
                "volume": unit_system.volume,
                "wind_speed": unit_system.wind_speed,
                "area": unit_system.area,
            },
            "location_name": self.name,
            "time_zone": self.time_zone,
            "components": components,
            "config_dir": "/config",
            "allowlist_external_dirs": self.allowlist_external_dirs,
            "allowlist_external_urls": self.allowlist_external_urls,
            "version": version,
            "config_source": "yaml",
            "recovery_mode": false,
            "safe_mode": false,
            "state": "RUNNING",
            "external_url": self.external_url,
            "internal_url": self.internal_url,
            "currency": self.currency,
            "country": self.country,
            "language": self.language,
            "radius": self.radius,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CoreConfig::default();
        assert_eq!(config.name, "Home");
        assert_eq!(config.latitude, 0.0);
        assert_eq!(config.time_zone, "UTC");
    }

    #[test]
    fn test_metric_unit_system() {
        let units = UnitSystem::metric();
        assert_eq!(units.temperature, "°C");
        assert_eq!(units.mass, "g");
        assert_eq!(units.wind_speed, "m/s");
    }

    #[test]
    fn test_imperial_unit_system() {
        let units = UnitSystem::imperial();
        assert_eq!(units.temperature, "°F");
        assert_eq!(units.mass, "lb");
        assert_eq!(units.wind_speed, "mph");
    }

    #[test]
    fn test_parse_from_yaml() {
        let yaml: Value = serde_yaml::from_str(
            r#"
homeassistant:
  name: Test Home
  latitude: 51.5074
  longitude: -0.1278
  elevation: 11
  unit_system: metric
  time_zone: UTC
  currency: USD
  country: US
"#,
        )
        .unwrap();

        let config = CoreConfig::from_yaml(&yaml).unwrap();
        assert_eq!(config.name, "Test Home");
        assert_eq!(config.latitude, 51.5074);
        assert_eq!(config.longitude, -0.1278);
        assert_eq!(config.elevation, 11);
        assert_eq!(config.time_zone, "UTC");
        assert_eq!(config.currency, "USD");
        assert_eq!(config.country, Some("US".to_string()));
    }

    #[test]
    fn test_unit_system_from_string() {
        let yaml: Value = serde_yaml::from_str(
            r#"
homeassistant:
  unit_system: imperial
"#,
        )
        .unwrap();

        let config = CoreConfig::from_yaml(&yaml).unwrap();
        let units = config.unit_system();
        assert_eq!(units.temperature, "°F");
    }

    #[test]
    fn test_to_api_response() {
        let config = CoreConfig {
            name: "Test".to_string(),
            latitude: 1.0,
            longitude: 2.0,
            elevation: 10,
            ..Default::default()
        };

        let response = config.to_api_response("2026.1.1", &["api".to_string()]);
        assert_eq!(response["location_name"], "Test");
        assert_eq!(response["latitude"], 1.0);
        assert_eq!(response["elevation"], 10);
    }
}
