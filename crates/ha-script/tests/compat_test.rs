//! Compatibility tests for script action types against Home Assistant configs
//!
//! These tests use real configuration examples from Home Assistant's test suite
//! (vendor/ha-core/tests/helpers/test_script.py)
//! to verify our Rust types correctly parse HA-format configurations.

use ha_script::Action;
use serde_json::json;

// ============================================================================
// Action Compatibility Tests (from HA's test_script.py)
// ============================================================================

#[test]
fn test_ha_compat_service_action() {
    let config = json!({
        "service": "light.turn_on",
        "target": {
            "entity_id": ["light.kitchen", "light.living_room"]
        },
        "data": {
            "brightness": 255
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Service(s) = action {
        assert_eq!(s.service, "light.turn_on");
        assert!(s.target.is_some());
    } else {
        panic!("Expected Service action");
    }
}

#[test]
fn test_ha_compat_delay_action() {
    let config = json!({
        "delay": {
            "hours": 0,
            "minutes": 5,
            "seconds": 0,
            "milliseconds": 0
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Delay(d) = action {
        assert!(d.delay.to_duration().is_some());
        assert_eq!(d.delay.to_duration().unwrap().as_secs(), 300);
    } else {
        panic!("Expected Delay action");
    }
}

#[test]
fn test_ha_compat_delay_template() {
    let config = json!({
        "delay": "{{ states('input_number.delay_minutes') | int * 60 }}"
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Delay(d) = action {
        // Template delay returns None for to_duration
        assert!(d.delay.to_duration().is_none());
    } else {
        panic!("Expected Delay action");
    }
}

#[test]
fn test_ha_compat_wait_template_action() {
    let config = json!({
        "wait_template": "{{ is_state('input_boolean.test', 'on') }}",
        "timeout": "00:01:00",
        "continue_on_timeout": true
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::WaitTemplate(w) = action {
        assert!(w.wait_template.contains("is_state"));
        assert!(w.continue_on_timeout);
    } else {
        panic!("Expected WaitTemplate action");
    }
}

#[test]
fn test_ha_compat_variables_action() {
    let config = json!({
        "variables": {
            "my_var": "{{ states('sensor.temp') }}",
            "brightness": 128
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Variables(v) = action {
        assert!(v.variables.contains_key("my_var"));
        assert!(v.variables.contains_key("brightness"));
    } else {
        panic!("Expected Variables action");
    }
}

#[test]
fn test_ha_compat_choose_action() {
    let config = json!({
        "choose": [
            {
                "conditions": [
                    {
                        "condition": "state",
                        "entity_id": "input_boolean.test",
                        "state": "on"
                    }
                ],
                "sequence": [
                    {"service": "light.turn_on", "target": {"entity_id": ["light.kitchen"]}}
                ]
            }
        ],
        "default": [
            {"service": "light.turn_off", "target": {"entity_id": ["light.kitchen"]}}
        ]
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Choose(c) = action {
        assert_eq!(c.choose.len(), 1);
        assert_eq!(c.default.len(), 1);
    } else {
        panic!("Expected Choose action");
    }
}

#[test]
fn test_ha_compat_choose_template_condition() {
    // HA allows template string as shorthand for conditions
    let config = json!({
        "choose": [
            {
                "conditions": "{{ is_state('light.test', 'on') }}",
                "sequence": [
                    {"service": "light.turn_off", "target": {"entity_id": ["light.test"]}}
                ]
            }
        ]
    });

    let action: Action = serde_json::from_value(config).unwrap();
    assert!(matches!(action, Action::Choose(_)));
}

#[test]
fn test_ha_compat_if_then_else_action() {
    let config = json!({
        "if": [
            {
                "condition": "state",
                "entity_id": "input_boolean.test",
                "state": "on"
            }
        ],
        "then": [
            {"service": "light.turn_on", "target": {"entity_id": ["light.test"]}}
        ],
        "else": [
            {"service": "light.turn_off", "target": {"entity_id": ["light.test"]}}
        ]
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::If(i) = action {
        assert_eq!(i.then.len(), 1);
        assert_eq!(i.r#else.len(), 1);
    } else {
        panic!("Expected If action");
    }
}

#[test]
fn test_ha_compat_repeat_count_action() {
    let config = json!({
        "repeat": {
            "count": 5,
            "sequence": [
                {"service": "light.toggle", "target": {"entity_id": ["light.test"]}}
            ]
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    assert!(matches!(action, Action::Repeat(_)));
}

#[test]
fn test_ha_compat_repeat_while_action() {
    let config = json!({
        "repeat": {
            "while": [
                {
                    "condition": "state",
                    "entity_id": "input_boolean.running",
                    "state": "on"
                }
            ],
            "sequence": [
                {"delay": {"seconds": 1}}
            ]
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    assert!(matches!(action, Action::Repeat(_)));
}

#[test]
fn test_ha_compat_repeat_until_action() {
    let config = json!({
        "repeat": {
            "until": [
                {
                    "condition": "state",
                    "entity_id": "input_boolean.done",
                    "state": "on"
                }
            ],
            "sequence": [
                {"delay": {"seconds": 1}}
            ]
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    assert!(matches!(action, Action::Repeat(_)));
}

#[test]
fn test_ha_compat_repeat_for_each_action() {
    let config = json!({
        "repeat": {
            "for_each": ["light.one", "light.two", "light.three"],
            "sequence": [
                {"service": "light.turn_on", "target": {"entity_id": ["{{ repeat.item }}"]}}
            ]
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    assert!(matches!(action, Action::Repeat(_)));
}

#[test]
fn test_ha_compat_parallel_action() {
    let config = json!({
        "parallel": [
            {"service": "light.turn_on", "target": {"entity_id": ["light.one"]}},
            {"service": "light.turn_on", "target": {"entity_id": ["light.two"]}},
            {"service": "light.turn_on", "target": {"entity_id": ["light.three"]}}
        ]
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Parallel(p) = action {
        assert_eq!(p.parallel.len(), 3);
    } else {
        panic!("Expected Parallel action");
    }
}

#[test]
fn test_ha_compat_sequence_action() {
    let config = json!({
        "sequence": [
            {"service": "light.turn_on", "target": {"entity_id": ["light.test"]}},
            {"delay": {"seconds": 5}},
            {"service": "light.turn_off", "target": {"entity_id": ["light.test"]}}
        ]
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Sequence(s) = action {
        assert_eq!(s.sequence.len(), 3);
    } else {
        panic!("Expected Sequence action");
    }
}

#[test]
fn test_ha_compat_stop_action() {
    let config = json!({
        "stop": "Script completed successfully",
        "error": false
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Stop(s) = action {
        assert_eq!(s.stop, "Script completed successfully");
        assert!(!s.error);
    } else {
        panic!("Expected Stop action");
    }
}

#[test]
fn test_ha_compat_stop_with_error() {
    let config = json!({
        "stop": "Something went wrong",
        "error": true
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Stop(s) = action {
        assert!(s.error);
    } else {
        panic!("Expected Stop action");
    }
}

#[test]
fn test_ha_compat_event_action() {
    let config = json!({
        "event": "my_custom_event",
        "event_data": {
            "message": "Hello World"
        }
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Event(e) = action {
        assert_eq!(e.event, "my_custom_event");
    } else {
        panic!("Expected Event action");
    }
}

#[test]
fn test_ha_compat_scene_action() {
    let config = json!({
        "scene": "scene.romantic_dinner"
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Scene(s) = action {
        assert_eq!(s.scene, "scene.romantic_dinner");
    } else {
        panic!("Expected Scene action");
    }
}

#[test]
fn test_ha_compat_wait_for_trigger_action() {
    let config = json!({
        "wait_for_trigger": [
            {
                "trigger": "state",
                "entity_id": "binary_sensor.motion",
                "to": "on"
            }
        ],
        "timeout": "00:00:30",
        "continue_on_timeout": true
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::WaitForTrigger(w) = action {
        assert_eq!(w.wait_for_trigger.len(), 1);
        assert!(w.continue_on_timeout);
    } else {
        panic!("Expected WaitForTrigger action");
    }
}

#[test]
fn test_ha_compat_service_with_response_variable() {
    let config = json!({
        "service": "weather.get_forecasts",
        "target": {"entity_id": ["weather.home"]},
        "data": {"type": "daily"},
        "response_variable": "forecast"
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Service(s) = action {
        assert_eq!(s.response_variable, Some("forecast".to_string()));
    } else {
        panic!("Expected Service action");
    }
}

#[test]
fn test_ha_compat_disabled_action() {
    let config = json!({
        "service": "light.turn_on",
        "target": {"entity_id": ["light.test"]},
        "enabled": false
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Service(s) = action {
        assert!(!s.enabled);
    } else {
        panic!("Expected Service action");
    }
}

#[test]
fn test_ha_compat_action_with_alias() {
    let config = json!({
        "alias": "Turn on kitchen light",
        "service": "light.turn_on",
        "target": {"entity_id": ["light.kitchen"]}
    });

    let action: Action = serde_json::from_value(config).unwrap();
    if let Action::Service(s) = action {
        assert_eq!(s.alias, Some("Turn on kitchen light".to_string()));
    } else {
        panic!("Expected Service action");
    }
}
