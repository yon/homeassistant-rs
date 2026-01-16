//! Global functions and objects for Home Assistant templates
//!
//! Provides functions like now(), utcnow(), states(), is_state(), etc.

use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use minijinja::value::{Kwargs, Value};
use minijinja::{Error, ErrorKind};
use std::convert::TryFrom;

/// Helper to convert Value to f64
fn value_to_f64(value: &Value) -> Option<f64> {
    f64::try_from(value.clone())
        .ok()
        .or_else(|| value.as_i64().map(|i| i as f64))
}

/// Helper to convert Value to bool
#[allow(dead_code)]
fn value_to_bool(value: &Value) -> Option<bool> {
    bool::try_from(value.clone()).ok()
}

// ==================== Time Functions ====================

/// Get the current local time
pub fn now() -> Value {
    Value::from_object(DateTimeWrapper(Local::now().with_timezone(&Utc)))
}

/// Get the current UTC time
pub fn utcnow() -> Value {
    Value::from_object(DateTimeWrapper(Utc::now()))
}

/// Convert a time string to today's datetime
pub fn today_at(time_str: &str) -> Result<Value, Error> {
    let today = Local::now().date_naive();

    // Parse time in HH:MM or HH:MM:SS format
    let time = chrono::NaiveTime::parse_from_str(time_str, "%H:%M:%S")
        .or_else(|_| chrono::NaiveTime::parse_from_str(time_str, "%H:%M"))
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("invalid time format: {}", e),
            )
        })?;

    let datetime = today.and_time(time);
    let local_dt = Local
        .from_local_datetime(&datetime)
        .single()
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "ambiguous local time"))?;

    Ok(Value::from_object(DateTimeWrapper(
        local_dt.with_timezone(&Utc),
    )))
}

/// Convert datetime to UNIX timestamp
pub fn as_timestamp(value: Value) -> Result<f64, Error> {
    if let Some(dt) = value.downcast_object_ref::<DateTimeWrapper>() {
        return Ok(dt.0.timestamp() as f64);
    }

    if let Some(s) = value.as_str() {
        // Try parsing ISO format
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(dt.timestamp() as f64);
        }
        // Try parsing common formats
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
            return Ok(dt.and_utc().timestamp() as f64);
        }
        // Try parsing as timestamp string
        if let Ok(ts) = s.parse::<f64>() {
            return Ok(ts);
        }
    }

    if let Some(i) = value.as_i64() {
        return Ok(i as f64);
    }

    if let Some(f) = value_to_f64(&value) {
        return Ok(f);
    }

    Err(Error::new(
        ErrorKind::InvalidOperation,
        "cannot convert to timestamp",
    ))
}

/// Convert timestamp to datetime
pub fn as_datetime(value: Value) -> Result<Value, Error> {
    let ts = if let Some(s) = value.as_str() {
        // Try parsing ISO format first
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(Value::from_object(DateTimeWrapper(dt.with_timezone(&Utc))));
        }
        // Try parsing as timestamp
        s.parse::<i64>()
            .map_err(|_| Error::new(ErrorKind::InvalidOperation, "cannot parse datetime string"))?
    } else if let Some(i) = value.as_i64() {
        i
    } else if let Some(f) = value_to_f64(&value) {
        f as i64
    } else {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            "cannot convert to datetime",
        ));
    };

    let dt = DateTime::from_timestamp(ts, 0)
        .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "invalid timestamp"))?;

    Ok(Value::from_object(DateTimeWrapper(dt)))
}

/// Convert datetime to local timezone
pub fn as_local(value: Value) -> Result<Value, Error> {
    if let Some(dt) = value.downcast_object_ref::<DateTimeWrapper>() {
        return Ok(Value::from_object(DateTimeWrapper(dt.0)));
    }

    // Try to parse as datetime first
    as_datetime(value)
}

/// Parse a datetime string with format
pub fn strptime(value: &str, format: &str) -> Result<Value, Error> {
    let dt = chrono::NaiveDateTime::parse_from_str(value, format).map_err(|e| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("failed to parse datetime: {}", e),
        )
    })?;

    Ok(Value::from_object(DateTimeWrapper(dt.and_utc())))
}

/// Create a timedelta
pub fn timedelta(kwargs: Kwargs) -> Result<Value, Error> {
    let days: i64 = kwargs.get::<Option<i64>>("days")?.unwrap_or(0);
    let hours: i64 = kwargs.get::<Option<i64>>("hours")?.unwrap_or(0);
    let minutes: i64 = kwargs.get::<Option<i64>>("minutes")?.unwrap_or(0);
    let seconds: i64 = kwargs.get::<Option<i64>>("seconds")?.unwrap_or(0);
    let milliseconds: i64 = kwargs.get::<Option<i64>>("milliseconds")?.unwrap_or(0);

    let duration = Duration::days(days)
        + Duration::hours(hours)
        + Duration::minutes(minutes)
        + Duration::seconds(seconds)
        + Duration::milliseconds(milliseconds);

    Ok(Value::from_object(TimeDeltaWrapper(duration)))
}

/// Convert string to timedelta
pub fn as_timedelta(value: &str) -> Result<Value, Error> {
    // Parse formats like "1:30:00" (hours:minutes:seconds) or "1 day, 2:30:00"
    let value = value.trim();

    let mut total_seconds: i64 = 0;

    // Check for "X days" prefix
    if let Some(rest) = value.strip_suffix(" days").or(value.strip_suffix(" day")) {
        if let Some((days_str, time_str)) = rest.split_once(", ") {
            total_seconds += days_str
                .trim()
                .parse::<i64>()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid timedelta"))?
                * 86400;
            return parse_time_to_seconds(time_str.trim()).map(|s| {
                Value::from_object(TimeDeltaWrapper(Duration::seconds(total_seconds + s)))
            });
        }
    }

    // Parse HH:MM:SS or MM:SS
    parse_time_to_seconds(value).map(|s| Value::from_object(TimeDeltaWrapper(Duration::seconds(s))))
}

fn parse_time_to_seconds(time_str: &str) -> Result<i64, Error> {
    let parts: Vec<&str> = time_str.split(':').collect();

    match parts.len() {
        2 => {
            let minutes: i64 = parts[0]
                .parse()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid timedelta"))?;
            let seconds: i64 = parts[1]
                .parse()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid timedelta"))?;
            Ok(minutes * 60 + seconds)
        }
        3 => {
            let hours: i64 = parts[0]
                .parse()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid timedelta"))?;
            let minutes: i64 = parts[1]
                .parse()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid timedelta"))?;
            let seconds: i64 = parts[2]
                .parse()
                .map_err(|_| Error::new(ErrorKind::InvalidOperation, "invalid timedelta"))?;
            Ok(hours * 3600 + minutes * 60 + seconds)
        }
        _ => Err(Error::new(
            ErrorKind::InvalidOperation,
            "invalid timedelta format",
        )),
    }
}

/// Get human-readable relative time
pub fn relative_time(value: Value) -> Result<String, Error> {
    let dt = if let Some(wrapper) = value.downcast_object_ref::<DateTimeWrapper>() {
        wrapper.0
    } else {
        let ts = as_timestamp(value)?;
        DateTime::from_timestamp(ts as i64, 0)
            .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "invalid timestamp"))?
    };

    let now = Utc::now();
    let diff = now.signed_duration_since(dt);

    Ok(format_duration(diff))
}

/// Format time since (positive duration)
pub fn time_since(value: Value) -> Result<String, Error> {
    relative_time(value)
}

/// Format time until (negative duration)
pub fn time_until(value: Value) -> Result<String, Error> {
    let dt = if let Some(wrapper) = value.downcast_object_ref::<DateTimeWrapper>() {
        wrapper.0
    } else {
        let ts = as_timestamp(value)?;
        DateTime::from_timestamp(ts as i64, 0)
            .ok_or_else(|| Error::new(ErrorKind::InvalidOperation, "invalid timestamp"))?
    };

    let now = Utc::now();
    let diff = dt.signed_duration_since(now);

    Ok(format_duration(diff))
}

fn format_duration(diff: Duration) -> String {
    let total_seconds = diff.num_seconds().abs();

    if total_seconds < 60 {
        let secs = total_seconds;
        if secs == 1 {
            "1 second".to_string()
        } else {
            format!("{} seconds", secs)
        }
    } else if total_seconds < 3600 {
        let mins = total_seconds / 60;
        if mins == 1 {
            "1 minute".to_string()
        } else {
            format!("{} minutes", mins)
        }
    } else if total_seconds < 86400 {
        let hours = total_seconds / 3600;
        if hours == 1 {
            "1 hour".to_string()
        } else {
            format!("{} hours", hours)
        }
    } else {
        let days = total_seconds / 86400;
        if days == 1 {
            "1 day".to_string()
        } else {
            format!("{} days", days)
        }
    }
}

// ==================== Utility Functions ====================

/// Immediate if - ternary operator as function
pub fn iif(
    condition: Value,
    if_true: Option<Value>,
    if_false: Option<Value>,
    if_none: Option<Value>,
) -> Value {
    if condition.is_none() || condition.is_undefined() {
        if_none.unwrap_or_else(|| if_false.clone().unwrap_or(Value::UNDEFINED))
    } else if condition.is_true() {
        if_true.unwrap_or(Value::from(true))
    } else {
        if_false.unwrap_or(Value::from(false))
    }
}

/// Calculate distance between two points
pub fn distance(lat1: f64, lon1: f64, lat2: Option<f64>, lon2: Option<f64>) -> Result<f64, Error> {
    // If only two args, assume second point is home (0, 0 as placeholder)
    let (lat2, lon2) = match (lat2, lon2) {
        (Some(lat), Some(lon)) => (lat, lon),
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                "distance requires 4 arguments (lat1, lon1, lat2, lon2)",
            ))
        }
    };

    // Haversine formula
    let r = 6371.0; // Earth's radius in km

    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();

    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();

    let a = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    Ok(r * c)
}

/// Get the type of a value
pub fn typeof_fn(value: Value) -> &'static str {
    use minijinja::value::ValueKind;

    if value.is_undefined() {
        "undefined"
    } else if value.is_none() {
        "none"
    } else {
        // Use kind() for accurate type detection
        match value.kind() {
            ValueKind::Bool => "boolean",
            ValueKind::String => "string",
            ValueKind::Number => {
                // Distinguish integer from float
                if value.as_i64().is_some() && value.to_string().parse::<i64>().is_ok() {
                    "integer"
                } else {
                    "float"
                }
            }
            ValueKind::Seq | ValueKind::Iterable => "list",
            ValueKind::Map => "mapping",
            ValueKind::Bytes => "bytes",
            ValueKind::Plain => {
                // Plain objects (like our DateTimeWrapper) - check if it's a mapping by trying to iterate keys
                if value.as_object().is_some() {
                    "mapping"
                } else {
                    "unknown"
                }
            }
            _ => "unknown",
        }
    }
}

/// Return a range of numbers
pub fn range_fn(start: i64, stop: Option<i64>, step: Option<i64>) -> Vec<i64> {
    let (start, stop) = match stop {
        Some(s) => (start, s),
        None => (0, start),
    };
    let step = step.unwrap_or(1);

    if step == 0 {
        return vec![];
    }

    let mut result = Vec::new();
    let mut i = start;

    if step > 0 {
        while i < stop {
            result.push(i);
            i += step;
        }
    } else {
        while i > stop {
            result.push(i);
            i += step;
        }
    }

    result
}

// ==================== DateTime Wrapper ====================

/// Wrapper for DateTime to expose to templates
#[derive(Debug, Clone)]
pub struct DateTimeWrapper(pub DateTime<Utc>);

impl std::fmt::Display for DateTimeWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d %H:%M:%S%.f%:z"))
    }
}

impl minijinja::value::Object for DateTimeWrapper {
    fn get_value(self: &std::sync::Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        match key {
            "year" => Some(Value::from(self.0.year())),
            "month" => Some(Value::from(self.0.month())),
            "day" => Some(Value::from(self.0.day())),
            "hour" => Some(Value::from(self.0.hour())),
            "minute" => Some(Value::from(self.0.minute())),
            "second" => Some(Value::from(self.0.second())),
            "microsecond" => Some(Value::from(self.0.timestamp_subsec_micros())),
            "weekday" => Some(Value::from(self.0.weekday().num_days_from_monday())),
            "isoweekday" => Some(Value::from(self.0.weekday().number_from_monday())),
            "timestamp" => Some(Value::from(self.0.timestamp())),
            _ => None,
        }
    }

    fn call_method(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[Value],
    ) -> Result<Value, Error> {
        match name {
            "strftime" => {
                let format = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidOperation,
                        "strftime requires format string",
                    )
                })?;
                Ok(Value::from(self.0.format(format).to_string()))
            }
            "timestamp" => Ok(Value::from(self.0.timestamp())),
            "isoformat" => Ok(Value::from(self.0.to_rfc3339())),
            "weekday" => Ok(Value::from(self.0.weekday().num_days_from_monday())),
            // Arithmetic methods for datetime
            "add" => {
                // DateTime.add(timedelta) -> DateTime
                let delta = args.first().ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidOperation,
                        "add requires a timedelta argument",
                    )
                })?;
                if let Some(td) = delta.downcast_object_ref::<TimeDeltaWrapper>() {
                    Ok(Value::from_object(DateTimeWrapper(self.0 + td.0)))
                } else {
                    Err(Error::new(
                        ErrorKind::InvalidOperation,
                        "can only add timedelta to datetime",
                    ))
                }
            }
            "sub" => {
                // DateTime.sub(timedelta) -> DateTime or DateTime.sub(datetime) -> TimeDelta
                let other = args.first().ok_or_else(|| {
                    Error::new(ErrorKind::InvalidOperation, "sub requires an argument")
                })?;
                if let Some(td) = other.downcast_object_ref::<TimeDeltaWrapper>() {
                    Ok(Value::from_object(DateTimeWrapper(self.0 - td.0)))
                } else if let Some(dt) = other.downcast_object_ref::<DateTimeWrapper>() {
                    Ok(Value::from_object(TimeDeltaWrapper(self.0 - dt.0)))
                } else {
                    Err(Error::new(
                        ErrorKind::InvalidOperation,
                        "can only subtract timedelta or datetime from datetime",
                    ))
                }
            }
            // Comparison methods for datetime
            "gt" => {
                let other = args
                    .first()
                    .and_then(|v| v.downcast_object_ref::<DateTimeWrapper>());
                Ok(Value::from(other.map(|dt| self.0 > dt.0).unwrap_or(false)))
            }
            "ge" => {
                let other = args
                    .first()
                    .and_then(|v| v.downcast_object_ref::<DateTimeWrapper>());
                Ok(Value::from(other.map(|dt| self.0 >= dt.0).unwrap_or(false)))
            }
            "lt" => {
                let other = args
                    .first()
                    .and_then(|v| v.downcast_object_ref::<DateTimeWrapper>());
                Ok(Value::from(other.map(|dt| self.0 < dt.0).unwrap_or(false)))
            }
            "le" => {
                let other = args
                    .first()
                    .and_then(|v| v.downcast_object_ref::<DateTimeWrapper>());
                Ok(Value::from(other.map(|dt| self.0 <= dt.0).unwrap_or(false)))
            }
            "eq" => {
                let other = args
                    .first()
                    .and_then(|v| v.downcast_object_ref::<DateTimeWrapper>());
                Ok(Value::from(other.map(|dt| self.0 == dt.0).unwrap_or(false)))
            }
            _ => Err(Error::new(
                ErrorKind::InvalidOperation,
                format!("unknown method: {}", name),
            )),
        }
    }

    fn render(self: &std::sync::Arc<Self>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m-%d %H:%M:%S%.f%:z"))
    }

    fn repr(self: &std::sync::Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Plain
    }
}

use chrono::Datelike;
use chrono::Timelike;

// ==================== TimeDelta Wrapper ====================

/// Wrapper for Duration to expose to templates
#[derive(Debug, Clone)]
pub struct TimeDeltaWrapper(pub Duration);

impl std::fmt::Display for TimeDeltaWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let total_secs = self.0.num_seconds();
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        write!(f, "{}:{:02}:{:02}", hours, mins, secs)
    }
}

impl minijinja::value::Object for TimeDeltaWrapper {
    fn get_value(self: &std::sync::Arc<Self>, key: &Value) -> Option<Value> {
        let key = key.as_str()?;
        match key {
            "days" => Some(Value::from(self.0.num_days())),
            "seconds" => Some(Value::from(self.0.num_seconds() % 86400)),
            "total_seconds" => Some(Value::from(self.0.num_seconds())),
            _ => None,
        }
    }

    fn repr(self: &std::sync::Arc<Self>) -> minijinja::value::ObjectRepr {
        minijinja::value::ObjectRepr::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_returns_datetime() {
        let result = now();
        assert!(result.downcast_object_ref::<DateTimeWrapper>().is_some());
    }

    #[test]
    fn test_as_timestamp() {
        let dt = now();
        let ts = as_timestamp(dt).unwrap();
        assert!(ts > 0.0);
    }

    #[test]
    fn test_relative_time() {
        let past = Utc::now() - Duration::hours(2);
        let result = relative_time(Value::from_object(DateTimeWrapper(past))).unwrap();
        assert_eq!(result, "2 hours");
    }

    // Note: timedelta is tested via engine integration tests since Kwargs
    // cannot be easily constructed outside of minijinja's function call context

    #[test]
    fn test_iif() {
        assert_eq!(
            iif(
                Value::from(true),
                Some(Value::from("yes")),
                Some(Value::from("no")),
                None
            )
            .as_str(),
            Some("yes")
        );
        assert_eq!(
            iif(
                Value::from(false),
                Some(Value::from("yes")),
                Some(Value::from("no")),
                None
            )
            .as_str(),
            Some("no")
        );
    }

    #[test]
    fn test_distance() {
        // Distance from NYC to LA is approximately 3944 km
        let dist = distance(40.7128, -74.0060, Some(34.0522), Some(-118.2437)).unwrap();
        assert!(dist > 3900.0 && dist < 4000.0);
    }

    #[test]
    fn test_typeof() {
        assert_eq!(typeof_fn(Value::from(42)), "integer");
        assert_eq!(typeof_fn(Value::from(3.14)), "float");
        assert_eq!(typeof_fn(Value::from("hello")), "string");
        assert_eq!(typeof_fn(Value::from(true)), "boolean");
    }

    #[test]
    fn test_range() {
        assert_eq!(range_fn(5, None, None), vec![0, 1, 2, 3, 4]);
        assert_eq!(range_fn(1, Some(5), None), vec![1, 2, 3, 4]);
        assert_eq!(range_fn(0, Some(10), Some(2)), vec![0, 2, 4, 6, 8]);
    }
}
