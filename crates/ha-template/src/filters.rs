//! Custom Jinja2 filters for Home Assistant templates
//!
//! These filters extend minijinja with Home Assistant-specific functionality.

use minijinja::value::{Kwargs, Value};
use minijinja::{Error, ErrorKind, State};
use regex::Regex;
use std::convert::TryFrom;

/// Helper to convert Value to f64
fn value_to_f64(value: &Value) -> Option<f64> {
    f64::try_from(value.clone())
        .ok()
        .or_else(|| value.as_i64().map(|i| i as f64))
}

/// Helper to convert Value to bool
fn value_to_bool(value: &Value) -> Option<bool> {
    bool::try_from(value.clone()).ok()
}

// ==================== String Filters ====================

/// Convert a string to a slug
pub fn slugify(value: &str, kwargs: Kwargs) -> Result<String, Error> {
    let separator: String = kwargs
        .get::<Option<String>>("separator")?
        .unwrap_or_else(|| "_".to_string());
    Ok(slug::slugify(value).replace('-', &separator))
}

/// Replace matches of a regex pattern with a replacement string
pub fn regex_replace(value: &str, find: &str, replace: &str) -> Result<String, Error> {
    let re = Regex::new(find)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e)))?;
    Ok(re.replace_all(value, replace).to_string())
}

/// Find all matches of a regex pattern
pub fn regex_findall(value: &str, pattern: &str) -> Result<Value, Error> {
    let re = Regex::new(pattern)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e)))?;

    let matches: Vec<Value> = re
        .captures_iter(value)
        .map(|cap| {
            if cap.len() > 1 {
                // Return captured groups as a list
                let groups: Vec<Value> = cap
                    .iter()
                    .skip(1)
                    .map(|m| {
                        m.map(|m| Value::from(m.as_str()))
                            .unwrap_or(Value::UNDEFINED)
                    })
                    .collect();
                Value::from(groups)
            } else {
                // Return the whole match
                Value::from(cap.get(0).map(|m| m.as_str()).unwrap_or(""))
            }
        })
        .collect();

    Ok(Value::from(matches))
}

/// Test if a regex pattern matches
pub fn regex_match(value: &str, pattern: &str) -> Result<bool, Error> {
    let re = Regex::new(pattern)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid regex: {}", e)))?;
    Ok(re.is_match(value))
}

// ==================== Type Conversion Filters ====================

/// Convert value to float with optional default (positional or keyword)
pub fn to_float(value: Value, default: Option<Value>) -> Result<Value, Error> {
    // Handle empty string as needing default
    if value.is_undefined() || value.is_none() || (value.as_str() == Some("")) {
        return match default {
            Some(d) => {
                if let Some(f) = value_to_f64(&d) {
                    Ok(Value::from(f))
                } else {
                    Ok(Value::from(0.0))
                }
            }
            None => Ok(Value::from(0.0)),
        };
    }

    let result = if let Some(f) = value_to_f64(&value) {
        Some(f)
    } else if let Some(s) = value.as_str() {
        s.trim().parse::<f64>().ok()
    } else {
        None
    };

    match result {
        Some(f) => Ok(Value::from(f)),
        None => match default {
            Some(d) => {
                if let Some(f) = value_to_f64(&d) {
                    Ok(Value::from(f))
                } else {
                    Ok(Value::from(0.0))
                }
            }
            None => Err(Error::new(
                ErrorKind::InvalidOperation,
                "cannot convert to float",
            )),
        },
    }
}

/// Convert value to integer with optional default (positional or keyword)
pub fn to_int(value: Value, default: Option<Value>) -> Result<Value, Error> {
    // Handle empty string as needing default
    if value.is_undefined() || value.is_none() || (value.as_str() == Some("")) {
        return match default {
            Some(d) => {
                if let Some(i) = d.as_i64() {
                    Ok(Value::from(i))
                } else if let Some(f) = value_to_f64(&d) {
                    Ok(Value::from(f as i64))
                } else {
                    Ok(Value::from(0))
                }
            }
            None => Ok(Value::from(0)),
        };
    }

    let result = if let Some(i) = value.as_i64() {
        Some(i)
    } else if let Some(f) = value_to_f64(&value) {
        Some(f as i64)
    } else if let Some(s) = value.as_str() {
        // Try parsing as integer first
        s.trim().parse::<i64>().ok().or_else(|| {
            // If that fails, try parsing as float and truncate
            s.trim().parse::<f64>().ok().map(|f| f as i64)
        })
    } else {
        None
    };

    match result {
        Some(i) => Ok(Value::from(i)),
        None => match default {
            Some(d) => {
                if let Some(i) = d.as_i64() {
                    Ok(Value::from(i))
                } else if let Some(f) = value_to_f64(&d) {
                    Ok(Value::from(f as i64))
                } else {
                    Ok(Value::from(0))
                }
            }
            None => Err(Error::new(
                ErrorKind::InvalidOperation,
                "cannot convert to int",
            )),
        },
    }
}

/// Convert value to boolean
pub fn to_bool(value: Value, default: Option<bool>) -> bool {
    let default = default.unwrap_or(false);

    if value.is_undefined() || value.is_none() {
        return default;
    }

    if let Some(b) = value_to_bool(&value) {
        return b;
    }

    if let Some(s) = value.as_str() {
        return matches!(
            s.to_lowercase().as_str(),
            "true" | "yes" | "on" | "1" | "enable" | "enabled"
        );
    }

    if let Some(i) = value.as_i64() {
        return i != 0;
    }

    if let Some(f) = value_to_f64(&value) {
        return f != 0.0;
    }

    value.is_true()
}

// ==================== Type Checking Filters ====================

/// Check if value is a number (integer or float)
pub fn is_number(value: Value) -> bool {
    if value.as_i64().is_some() {
        return true;
    }
    if value_to_f64(&value).is_some() {
        return true;
    }
    // Also check if it's a string that can be parsed as a number
    if let Some(s) = value.as_str() {
        return s.trim().parse::<f64>().is_ok();
    }
    false
}

/// Check if value is a string
pub fn is_string(value: Value) -> bool {
    value.as_str().is_some()
}

/// Check if value is a list/sequence
pub fn is_list(value: Value) -> bool {
    // Check if it's iterable and not a string
    if value.as_str().is_some() {
        return false;
    }
    value.try_iter().is_ok()
}

/// Check if a value contains another value
pub fn contains(value: Value, search: Value) -> bool {
    // String contains
    if let (Some(haystack), Some(needle)) = (value.as_str(), search.as_str()) {
        return haystack.contains(needle);
    }

    // List/sequence contains
    if let Ok(iter) = value.try_iter() {
        for item in iter {
            // Compare string values (only if both are strings)
            if let (Some(a), Some(b)) = (item.as_str(), search.as_str()) {
                if a == b {
                    return true;
                }
            }
            // Compare integer values
            if let (Some(a), Some(b)) = (item.as_i64(), search.as_i64()) {
                if a == b {
                    return true;
                }
            }
            // Compare float values
            if let (Some(a), Some(b)) = (value_to_f64(&item), value_to_f64(&search)) {
                if (a - b).abs() < f64::EPSILON {
                    return true;
                }
            }
        }
    }

    false
}

// ==================== Math Filters ====================

/// Round a number to specified precision
pub fn round_filter(value: f64, precision: Option<i32>, kwargs: Kwargs) -> Result<f64, Error> {
    let precision = precision.unwrap_or(0);
    let method: String = kwargs
        .get::<Option<String>>("method")?
        .unwrap_or_else(|| "common".to_string());

    let multiplier = 10_f64.powi(precision);
    let scaled = value * multiplier;

    let rounded = match method.as_str() {
        "ceil" => scaled.ceil(),
        "floor" => scaled.floor(),
        "half" => (scaled * 2.0).round() / 2.0,
        _ => scaled.round(), // "common" or default
    };

    Ok(rounded / multiplier)
}

/// Clamp a value between min and max
#[allow(dead_code)]
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

/// Get the absolute value
pub fn abs_filter(value: f64) -> f64 {
    value.abs()
}

/// Calculate square root
pub fn sqrt(value: f64) -> Result<f64, Error> {
    if value < 0.0 {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            "cannot take square root of negative number",
        ))
    } else {
        Ok(value.sqrt())
    }
}

/// Calculate natural logarithm
pub fn log_filter(value: f64, base: Option<f64>) -> Result<f64, Error> {
    if value <= 0.0 {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "logarithm requires positive number",
        ));
    }

    match base {
        Some(b) => Ok(value.log(b)),
        None => Ok(value.ln()),
    }
}

/// Trigonometric functions
pub fn sin(value: f64) -> f64 {
    value.sin()
}

pub fn cos(value: f64) -> f64 {
    value.cos()
}

pub fn tan(value: f64) -> f64 {
    value.tan()
}

pub fn asin(value: f64) -> Result<f64, Error> {
    if !(-1.0..=1.0).contains(&value) {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            "asin requires value between -1 and 1",
        ))
    } else {
        Ok(value.asin())
    }
}

pub fn acos(value: f64) -> Result<f64, Error> {
    if !(-1.0..=1.0).contains(&value) {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            "acos requires value between -1 and 1",
        ))
    } else {
        Ok(value.acos())
    }
}

pub fn atan(value: f64) -> f64 {
    value.atan()
}

pub fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

// ==================== Aggregate Filters ====================

/// Calculate average of a sequence
pub fn average(_state: &State, values: Value, kwargs: Kwargs) -> Result<Value, Error> {
    let default: Option<f64> = kwargs.get::<Option<f64>>("default")?;

    let iter = match values.try_iter() {
        Ok(it) => it,
        Err(_) => {
            return Ok(default.map(Value::from).unwrap_or(Value::UNDEFINED));
        }
    };

    let mut sum = 0.0;
    let mut count = 0;

    for item in iter {
        if let Some(n) = value_to_f64(&item) {
            sum += n;
            count += 1;
        }
    }

    if count == 0 {
        Ok(default.map(Value::from).unwrap_or(Value::UNDEFINED))
    } else {
        Ok(Value::from(sum / count as f64))
    }
}

/// Calculate median of a sequence
pub fn median(_state: &State, values: Value) -> Result<Value, Error> {
    let iter = match values.try_iter() {
        Ok(it) => it,
        Err(_) => return Ok(Value::UNDEFINED),
    };

    let mut nums: Vec<f64> = iter.filter_map(|v| value_to_f64(&v)).collect();

    if nums.is_empty() {
        return Ok(Value::UNDEFINED);
    }

    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mid = nums.len() / 2;
    if nums.len() % 2 == 0 {
        Ok(Value::from((nums[mid - 1] + nums[mid]) / 2.0))
    } else {
        Ok(Value::from(nums[mid]))
    }
}

// ==================== JSON Filters ====================

/// Convert value to JSON string
pub fn to_json(value: Value, kwargs: Kwargs) -> Result<String, Error> {
    let pretty: bool = kwargs.get::<Option<bool>>("pretty")?.unwrap_or(false);

    // Convert minijinja Value to serde_json Value
    let json_value = value_to_json(&value)?;

    if pretty {
        serde_json::to_string_pretty(&json_value)
    } else {
        serde_json::to_string(&json_value)
    }
    .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("JSON error: {}", e)))
}

/// Parse JSON string to value
pub fn from_json(value: &str) -> Result<Value, Error> {
    let json: serde_json::Value = serde_json::from_str(value)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid JSON: {}", e)))?;

    Ok(json_to_value(&json))
}

fn value_to_json(value: &Value) -> Result<serde_json::Value, Error> {
    if value.is_undefined() || value.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Some(b) = value_to_bool(value) {
        Ok(serde_json::Value::Bool(b))
    } else if let Some(i) = value.as_i64() {
        Ok(serde_json::json!(i))
    } else if let Some(f) = value_to_f64(value) {
        Ok(serde_json::json!(f))
    } else if let Some(s) = value.as_str() {
        Ok(serde_json::Value::String(s.to_string()))
    } else if value.as_object().is_some() {
        // Try to handle as a map/dict first (object with string keys)
        if let Ok(iter) = value.try_iter() {
            let keys: Vec<Value> = iter.collect();
            // Check if it looks like a map (all keys are strings)
            if keys.iter().all(|k| k.as_str().is_some()) {
                let mut map = serde_json::Map::new();
                for key in keys {
                    if let Some(k) = key.as_str() {
                        if let Ok(v) = value.get_item(&key) {
                            map.insert(k.to_string(), value_to_json(&v)?);
                        }
                    }
                }
                return Ok(serde_json::Value::Object(map));
            }
            // Otherwise treat as array
            let arr: Result<Vec<serde_json::Value>, Error> =
                keys.into_iter().map(|v| value_to_json(&v)).collect();
            Ok(serde_json::Value::Array(arr?))
        } else {
            // Object with no iteration support - serialize as string
            Ok(serde_json::Value::String(value.to_string()))
        }
    } else if let Ok(iter) = value.try_iter() {
        let arr: Result<Vec<serde_json::Value>, Error> = iter.map(|v| value_to_json(&v)).collect();
        Ok(serde_json::Value::Array(arr?))
    } else {
        // Try to serialize as string
        Ok(serde_json::Value::String(value.to_string()))
    }
}

fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::from(()),
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(arr) => {
            Value::from(arr.iter().map(json_to_value).collect::<Vec<_>>())
        }
        serde_json::Value::Object(obj) => {
            let map: std::collections::BTreeMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::from_object(map)
        }
    }
}

// ==================== Encoding Filters ====================

/// Base64 encode a string
pub fn base64_encode(value: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
}

/// Base64 decode a string
pub fn base64_decode(value: &str) -> Result<String, Error> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("invalid base64: {}", e),
            )
        })?;

    String::from_utf8(bytes)
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, format!("invalid UTF-8: {}", e)))
}

/// URL encode a string
pub fn urlencode(value: &str) -> String {
    urlencoding::encode(value).to_string()
}

/// Convert ordinal number
pub fn ordinal(value: i64) -> String {
    let suffix = match (value % 10, value % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{}{}", value, suffix)
}

// ==================== List Filters ====================

/// Flatten nested lists
pub fn flatten(values: Value, depth: Option<i32>) -> Result<Value, Error> {
    let depth = depth.unwrap_or(1);
    Ok(Value::from(flatten_recursive(&values, depth)?))
}

fn flatten_recursive(value: &Value, depth: i32) -> Result<Vec<Value>, Error> {
    let mut result = Vec::new();

    if let Ok(iter) = value.try_iter() {
        for item in iter {
            if depth > 0 {
                if item.try_iter().is_ok() {
                    result.extend(flatten_recursive(&item, depth - 1)?);
                } else {
                    result.push(item);
                }
            } else {
                result.push(item);
            }
        }
    } else {
        result.push(value.clone());
    }

    Ok(result)
}

// ==================== Tests (Jinja2 tests, not unit tests) ====================

/// Test if value matches regex
pub fn match_test(value: &str, pattern: &str) -> Result<bool, Error> {
    regex_match(value, pattern)
}

/// Test if value is defined (not undefined)
pub fn is_defined(value: Value) -> bool {
    !value.is_undefined()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_replace() {
        assert_eq!(
            regex_replace("hello world", r"\s+", "-").unwrap(),
            "hello-world"
        );
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5.0, 0.0, 10.0), 5.0);
        assert_eq!(clamp(-5.0, 0.0, 10.0), 0.0);
        assert_eq!(clamp(15.0, 0.0, 10.0), 10.0);
    }

    #[test]
    fn test_from_json() {
        let result = from_json(r#"{"key": "value"}"#).unwrap();
        // Check the value contains expected data
        assert!(!result.is_undefined());
    }

    #[test]
    fn test_ordinal() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(21), "21st");
    }

    #[test]
    fn test_is_number() {
        assert!(is_number(Value::from(42)));
        assert!(is_number(Value::from(3.5)));
        assert!(!is_number(Value::from("hello")));
    }
}
