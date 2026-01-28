//! Domain metadata and constants
//!
//! This module defines domain-level metadata for Home Assistant entity domains,
//! including which domains are read-only and what services each domain supports.

use std::collections::{HashMap, HashSet};

/// Domains that are read-only and should NOT have turn_on/turn_off/toggle services.
///
/// These domains represent entities that only report state and cannot be controlled.
pub static READONLY_DOMAINS: &[&str] = &[
    "sensor",
    "binary_sensor",
    "weather",
    "device_tracker",
    "sun",
    "zone",
    "person",
    "calendar",
    "image",
    "event",
    "update",
    "tts",
    "stt",
    "conversation",
];

/// Check if a domain is read-only (cannot have turn_on/turn_off/toggle services)
pub fn is_readonly_domain(domain: &str) -> bool {
    READONLY_DOMAINS.contains(&domain)
}

/// Get the set of read-only domains as a HashSet for efficient lookups
pub fn readonly_domains_set() -> HashSet<&'static str> {
    READONLY_DOMAINS.iter().copied().collect()
}

/// Default services for domains that have standard controllable behavior.
///
/// If a domain is not in this map and is not read-only, it defaults to
/// `["turn_on", "turn_off", "toggle"]`.
pub fn domain_default_services() -> HashMap<&'static str, Vec<&'static str>> {
    [
        // Media player has extensive controls
        (
            "media_player",
            vec![
                "turn_on",
                "turn_off",
                "toggle",
                "volume_up",
                "volume_down",
                "volume_set",
                "volume_mute",
                "media_play",
                "media_pause",
                "media_stop",
                "media_next_track",
                "media_previous_track",
                "media_seek",
                "select_source",
                "select_sound_mode",
                "play_media",
                "shuffle_set",
                "repeat_set",
            ],
        ),
        // Climate has HVAC controls
        (
            "climate",
            vec![
                "turn_on",
                "turn_off",
                "toggle",
                "set_hvac_mode",
                "set_preset_mode",
                "set_temperature",
                "set_humidity",
                "set_fan_mode",
                "set_swing_mode",
            ],
        ),
        // Cover has open/close/stop
        (
            "cover",
            vec![
                "open_cover",
                "close_cover",
                "stop_cover",
                "set_cover_position",
                "open_cover_tilt",
                "close_cover_tilt",
                "set_cover_tilt_position",
                "toggle",
                "toggle_cover_tilt",
            ],
        ),
        // Fan has speed controls
        (
            "fan",
            vec![
                "turn_on",
                "turn_off",
                "toggle",
                "set_percentage",
                "set_preset_mode",
                "set_direction",
                "oscillate",
            ],
        ),
        // Lock has lock/unlock
        ("lock", vec!["lock", "unlock", "open"]),
        // Vacuum has specific controls
        (
            "vacuum",
            vec![
                "turn_on",
                "turn_off",
                "toggle",
                "start",
                "pause",
                "stop",
                "return_to_base",
                "clean_spot",
                "locate",
                "set_fan_speed",
                "send_command",
            ],
        ),
        // Humidifier
        (
            "humidifier",
            vec!["turn_on", "turn_off", "toggle", "set_humidity", "set_mode"],
        ),
        // Water heater
        (
            "water_heater",
            vec![
                "turn_on",
                "turn_off",
                "set_temperature",
                "set_operation_mode",
            ],
        ),
        // Alarm control panel
        (
            "alarm_control_panel",
            vec![
                "alarm_arm_home",
                "alarm_arm_away",
                "alarm_arm_night",
                "alarm_arm_vacation",
                "alarm_arm_custom_bypass",
                "alarm_disarm",
                "alarm_trigger",
            ],
        ),
    ]
    .into_iter()
    .collect()
}

/// Get the default services for a domain.
///
/// - Returns `None` for read-only domains (they shouldn't have services)
/// - Returns domain-specific services if defined
/// - Returns `["turn_on", "turn_off", "toggle"]` for other controllable domains
pub fn get_domain_services(domain: &str) -> Option<Vec<&'static str>> {
    if is_readonly_domain(domain) {
        return None;
    }

    let services = domain_default_services();
    if let Some(domain_services) = services.get(domain) {
        Some(domain_services.clone())
    } else {
        // Default services for controllable domains
        Some(vec!["turn_on", "turn_off", "toggle"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_readonly_domains() {
        assert!(is_readonly_domain("sensor"));
        assert!(is_readonly_domain("binary_sensor"));
        assert!(is_readonly_domain("weather"));
        assert!(!is_readonly_domain("light"));
        assert!(!is_readonly_domain("switch"));
    }

    #[test]
    fn test_get_domain_services_readonly() {
        assert_eq!(get_domain_services("sensor"), None);
        assert_eq!(get_domain_services("binary_sensor"), None);
    }

    #[test]
    fn test_get_domain_services_specific() {
        let lock_services = get_domain_services("lock").unwrap();
        assert!(lock_services.contains(&"lock"));
        assert!(lock_services.contains(&"unlock"));
        assert!(!lock_services.contains(&"turn_on"));
    }

    #[test]
    fn test_get_domain_services_default() {
        let light_services = get_domain_services("light").unwrap();
        assert!(light_services.contains(&"turn_on"));
        assert!(light_services.contains(&"turn_off"));
        assert!(light_services.contains(&"toggle"));
    }
}
