//! Response comparison utilities

use crate::client::ApiResponse;
use crate::ws_client::WsTestResult;
use serde_json::Value;
use std::collections::HashSet;

/// Result of comparing two API responses
#[derive(Debug)]
pub struct ComparisonResult {
    pub endpoint: String,
    pub passed: bool,
    pub differences: Vec<Difference>,
}

/// A specific difference between responses
#[derive(Debug)]
pub struct Difference {
    pub category: DiffCategory,
    pub path: String,
    pub python_value: String,
    pub rust_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiffCategory {
    StatusCode,
    Header,
    BodyStructure,
    BodyValue,
    Missing,
    Extra,
}

impl std::fmt::Display for DiffCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffCategory::StatusCode => write!(f, "STATUS"),
            DiffCategory::Header => write!(f, "HEADER"),
            DiffCategory::BodyStructure => write!(f, "STRUCTURE"),
            DiffCategory::BodyValue => write!(f, "VALUE"),
            DiffCategory::Missing => write!(f, "MISSING"),
            DiffCategory::Extra => write!(f, "EXTRA"),
        }
    }
}

/// Options for comparing responses
#[derive(Debug, Clone, Default)]
pub struct CompareOptions {
    /// Fields to ignore when comparing (e.g., timestamps, IDs)
    pub ignore_fields: HashSet<String>,
    /// Whether to compare headers
    pub compare_headers: bool,
    /// Headers to ignore when comparing
    pub ignore_headers: HashSet<String>,
    /// Whether to allow extra fields in Rust response
    pub allow_extra_fields: bool,
    /// Sort arrays by this key before comparing (e.g., "entity_id")
    pub sort_arrays_by: Option<String>,
}

impl CompareOptions {
    pub fn new() -> Self {
        let mut opts = Self::default();
        // By default, ignore volatile fields
        opts.ignore_fields.insert("last_changed".to_string());
        opts.ignore_fields.insert("last_updated".to_string());
        opts.ignore_fields.insert("context.id".to_string());
        opts.ignore_fields.insert("id".to_string());

        // Ignore certain headers by default
        opts.ignore_headers.insert("date".to_string());
        opts.ignore_headers.insert("server".to_string());
        opts.ignore_headers.insert("content-length".to_string());

        opts
    }

    pub fn with_header_comparison(mut self) -> Self {
        self.compare_headers = true;
        self
    }

    pub fn ignore_field(mut self, field: &str) -> Self {
        self.ignore_fields.insert(field.to_string());
        self
    }

    pub fn sort_arrays_by(mut self, key: &str) -> Self {
        self.sort_arrays_by = Some(key.to_string());
        self
    }

    /// Check if a path should be ignored
    /// Matches if the path equals an ignored field OR ends with ".{ignored_field}"
    pub fn should_ignore(&self, path: &str) -> bool {
        let path = path.trim_start_matches('.');
        for field in &self.ignore_fields {
            if path == field {
                return true;
            }
            // Check if path ends with .field (e.g., "foo.bar.context.id" matches "context.id")
            if path.ends_with(&format!(".{}", field)) {
                return true;
            }
        }
        false
    }
}

/// Compare two API responses
pub fn compare_responses(
    endpoint: &str,
    python: &ApiResponse,
    rust: &ApiResponse,
    options: &CompareOptions,
) -> ComparisonResult {
    let mut differences = Vec::new();

    // Compare status codes
    if python.status != rust.status {
        differences.push(Difference {
            category: DiffCategory::StatusCode,
            path: "status_code".to_string(),
            python_value: python.status.to_string(),
            rust_value: rust.status.to_string(),
        });
    }

    // Compare headers if requested
    if options.compare_headers {
        compare_headers(python, rust, options, &mut differences);
    }

    // Compare body
    if let (Some(py_body), Some(rs_body)) = (&python.body, &rust.body) {
        compare_json("", py_body, rs_body, options, &mut differences);
    } else if python.body.is_some() != rust.body.is_some() {
        differences.push(Difference {
            category: DiffCategory::BodyStructure,
            path: "body".to_string(),
            python_value: if python.body.is_some() {
                "present"
            } else {
                "absent"
            }
            .to_string(),
            rust_value: if rust.body.is_some() {
                "present"
            } else {
                "absent"
            }
            .to_string(),
        });
    }

    ComparisonResult {
        endpoint: endpoint.to_string(),
        passed: differences.is_empty(),
        differences,
    }
}

fn compare_headers(
    python: &ApiResponse,
    rust: &ApiResponse,
    options: &CompareOptions,
    differences: &mut Vec<Difference>,
) {
    let py_headers: HashSet<_> = python
        .headers
        .iter()
        .filter(|(k, _)| !options.ignore_headers.contains(&k.to_lowercase()))
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect();

    let rs_headers: HashSet<_> = rust
        .headers
        .iter()
        .filter(|(k, _)| !options.ignore_headers.contains(&k.to_lowercase()))
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect();

    // Check for missing headers
    for (key, value) in &py_headers {
        if let Some((_, rs_value)) = rs_headers.iter().find(|(k, _)| k == key) {
            if value != rs_value {
                differences.push(Difference {
                    category: DiffCategory::Header,
                    path: format!("header.{}", key),
                    python_value: value.clone(),
                    rust_value: rs_value.clone(),
                });
            }
        } else {
            differences.push(Difference {
                category: DiffCategory::Missing,
                path: format!("header.{}", key),
                python_value: value.clone(),
                rust_value: "(missing)".to_string(),
            });
        }
    }
}

fn compare_json(
    path: &str,
    python: &Value,
    rust: &Value,
    options: &CompareOptions,
    differences: &mut Vec<Difference>,
) {
    // Check if this path should be ignored
    if options.should_ignore(path) {
        return;
    }

    match (python, rust) {
        (Value::Object(py_obj), Value::Object(rs_obj)) => {
            // Check all Python keys exist in Rust with same values
            for (key, py_value) in py_obj {
                let new_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };

                if options.should_ignore(&new_path) {
                    continue;
                }

                match rs_obj.get(key) {
                    Some(rs_value) => {
                        compare_json(&new_path, py_value, rs_value, options, differences);
                    }
                    None => {
                        differences.push(Difference {
                            category: DiffCategory::Missing,
                            path: new_path,
                            python_value: py_value.to_string(),
                            rust_value: "(missing)".to_string(),
                        });
                    }
                }
            }

            // Check for extra keys in Rust that aren't in Python
            if !options.allow_extra_fields {
                for key in rs_obj.keys() {
                    let new_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };

                    if !py_obj.contains_key(key) && !options.should_ignore(&new_path) {
                        differences.push(Difference {
                            category: DiffCategory::Extra,
                            path: new_path,
                            python_value: "(not present)".to_string(),
                            rust_value: rs_obj.get(key).unwrap().to_string(),
                        });
                    }
                }
            }
        }
        (Value::Array(py_arr), Value::Array(rs_arr)) => {
            // Sort arrays if sort key is specified
            let (py_sorted, rs_sorted) = if let Some(ref sort_key) = options.sort_arrays_by {
                let mut py_vec: Vec<_> = py_arr.iter().collect();
                let mut rs_vec: Vec<_> = rs_arr.iter().collect();

                py_vec.sort_by(|a, b| {
                    let a_key = a.get(sort_key).and_then(|v| v.as_str()).unwrap_or("");
                    let b_key = b.get(sort_key).and_then(|v| v.as_str()).unwrap_or("");
                    a_key.cmp(b_key)
                });
                rs_vec.sort_by(|a, b| {
                    let a_key = a.get(sort_key).and_then(|v| v.as_str()).unwrap_or("");
                    let b_key = b.get(sort_key).and_then(|v| v.as_str()).unwrap_or("");
                    a_key.cmp(b_key)
                });

                (py_vec, rs_vec)
            } else {
                (py_arr.iter().collect(), rs_arr.iter().collect())
            };

            if py_sorted.len() != rs_sorted.len() {
                differences.push(Difference {
                    category: DiffCategory::BodyStructure,
                    path: format!("{}.length", path),
                    python_value: py_sorted.len().to_string(),
                    rust_value: rs_sorted.len().to_string(),
                });
            }

            // Compare elements
            for (i, (py_elem, rs_elem)) in py_sorted.iter().zip(rs_sorted.iter()).enumerate() {
                let new_path = format!("{}[{}]", path, i);
                compare_json(&new_path, py_elem, rs_elem, options, differences);
            }
        }
        _ => {
            // Compare primitive values
            if python != rust {
                differences.push(Difference {
                    category: DiffCategory::BodyValue,
                    path: path.to_string(),
                    python_value: python.to_string(),
                    rust_value: rust.to_string(),
                });
            }
        }
    }
}

impl ComparisonResult {
    /// Print a summary of the comparison
    pub fn print_summary(&self) {
        if self.passed {
            println!("✅ {} - PASS", self.endpoint);
        } else {
            println!(
                "❌ {} - FAIL ({} differences)",
                self.endpoint,
                self.differences.len()
            );
            for diff in &self.differences {
                println!(
                    "   [{:>9}] {} : Python={} Rust={}",
                    diff.category, diff.path, diff.python_value, diff.rust_value
                );
            }
        }
    }
}

/// Result of comparing WebSocket test results
#[derive(Debug)]
pub struct WsComparisonResult {
    pub test_name: String,
    pub passed: bool,
    pub python_error: Option<String>,
    pub rust_error: Option<String>,
    pub differences: Vec<Difference>,
}

impl WsComparisonResult {
    /// Print a summary of the comparison
    pub fn print_summary(&self) {
        if self.passed {
            println!("✅ ws:{} - PASS", self.test_name);
        } else if let Some(ref py_err) = self.python_error {
            println!("⚠️  ws:{} - Python error: {}", self.test_name, py_err);
        } else if let Some(ref rs_err) = self.rust_error {
            println!("❌ ws:{} - Rust error: {}", self.test_name, rs_err);
        } else {
            println!(
                "❌ ws:{} - FAIL ({} differences)",
                self.test_name,
                self.differences.len()
            );
            for diff in &self.differences {
                println!(
                    "   [{:>9}] {} : Python={} Rust={}",
                    diff.category, diff.path, diff.python_value, diff.rust_value
                );
            }
        }
    }
}

/// Compare two WebSocket test results
pub fn compare_ws_results(
    test_name: &str,
    python: &WsTestResult,
    rust: &WsTestResult,
    options: &CompareOptions,
) -> WsComparisonResult {
    // Check for errors
    if let Some(ref err) = python.error {
        return WsComparisonResult {
            test_name: test_name.to_string(),
            passed: false,
            python_error: Some(err.clone()),
            rust_error: None,
            differences: Vec::new(),
        };
    }
    if let Some(ref err) = rust.error {
        return WsComparisonResult {
            test_name: test_name.to_string(),
            passed: false,
            python_error: None,
            rust_error: Some(err.clone()),
            differences: Vec::new(),
        };
    }

    // Compare exchanges
    let mut differences = Vec::new();

    if python.exchanges.len() != rust.exchanges.len() {
        differences.push(Difference {
            category: DiffCategory::BodyStructure,
            path: "exchanges.length".to_string(),
            python_value: python.exchanges.len().to_string(),
            rust_value: rust.exchanges.len().to_string(),
        });
    }

    // Compare each exchange
    for (i, (py_ex, rs_ex)) in python
        .exchanges
        .iter()
        .zip(rust.exchanges.iter())
        .enumerate()
    {
        // Compare responses (requests are identical by construction)
        let path = format!("exchange[{}].response", i);
        compare_json(
            &path,
            &py_ex.response,
            &rs_ex.response,
            options,
            &mut differences,
        );
    }

    WsComparisonResult {
        test_name: test_name.to_string(),
        passed: differences.is_empty(),
        python_error: None,
        rust_error: None,
        differences,
    }
}
