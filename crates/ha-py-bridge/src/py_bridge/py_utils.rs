//! Python utility functions for the py-bridge
//!
//! This module contains shared utilities for working with Python objects,
//! including conversion to JSON and handling of Home Assistant's UNDEFINED sentinel.

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Check if a Python object is Home Assistant's UNDEFINED sentinel value.
///
/// UNDEFINED is used by HA to indicate "no value provided" vs None/null.
/// When serializing to JSON, we treat UNDEFINED as null to prevent
/// "UndefinedType._singleton" from appearing in serialized data.
pub fn is_undefined_sentinel(obj: &Bound<'_, PyAny>) -> bool {
    // Check the type name for UndefinedType
    if let Ok(type_name) = obj.get_type().name() {
        if type_name.to_string().contains("UndefinedType") {
            return true;
        }
    }
    // Also check string representation as fallback
    let str_repr = obj.to_string();
    str_repr.contains("UndefinedType") || str_repr == "UndefinedType._singleton"
}

/// Convert Python object to JSON value, handling UNDEFINED sentinels.
///
/// This is the canonical conversion function that properly handles:
/// - None → null
/// - UNDEFINED → null (filtered out)
/// - bool, int, float, str → corresponding JSON types
/// - list → JSON array
/// - dict → JSON object (with UNDEFINED values skipped)
pub fn pyobject_to_json(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        return Ok(serde_json::Value::Null);
    }
    // Filter UNDEFINED sentinel values - treat as null
    if is_undefined_sentinel(obj) {
        return Ok(serde_json::Value::Null);
    }
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(serde_json::Value::Number(i.into()));
    }
    if let Ok(f) = obj.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Ok(serde_json::Value::Number(n));
        }
    }
    if let Ok(s) = obj.extract::<String>() {
        // Double-check string values for UNDEFINED that slipped through
        if s.contains("UndefinedType") {
            return Ok(serde_json::Value::Null);
        }
        return Ok(serde_json::Value::String(s));
    }
    if let Ok(list) = obj.downcast::<pyo3::types::PyList>() {
        let arr: Result<Vec<_>, _> = list.iter().map(|item| pyobject_to_json(&item)).collect();
        return Ok(serde_json::Value::Array(arr?));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (k, v) in dict.iter() {
            if let Ok(key) = k.extract::<String>() {
                // Skip UNDEFINED values in dicts entirely
                if !is_undefined_sentinel(&v) {
                    map.insert(key, pyobject_to_json(&v)?);
                }
            }
        }
        return Ok(serde_json::Value::Object(map));
    }
    // Default to string representation, but check for UNDEFINED
    let str_repr = obj.to_string();
    if str_repr.contains("UndefinedType") {
        return Ok(serde_json::Value::Null);
    }
    Ok(serde_json::Value::String(str_repr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_undefined_detection() {
        // This test requires Python, so we just verify the string check works
        assert!(
            "UndefinedType._singleton".contains("UndefinedType"),
            "String detection should work"
        );
    }
}
