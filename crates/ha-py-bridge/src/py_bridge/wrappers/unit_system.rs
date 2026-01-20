//! UnitSystemWrapper - unit system configuration for Python

use pyo3::prelude::*;

/// Python wrapper for Home Assistant unit system
#[pyclass(name = "UnitSystemWrapper")]
pub struct UnitSystemWrapper {
    #[pyo3(get)]
    pub length_unit: String,
    #[pyo3(get)]
    pub temperature_unit: String,
    #[pyo3(get)]
    pub mass_unit: String,
    #[pyo3(get)]
    pub volume_unit: String,
    #[pyo3(get)]
    pub pressure_unit: String,
    #[pyo3(get)]
    pub wind_speed_unit: String,
    #[pyo3(get)]
    pub accumulated_precipitation_unit: String,
    is_metric: bool,
}

impl UnitSystemWrapper {
    pub fn metric() -> Self {
        Self {
            length_unit: "km".to_string(),
            temperature_unit: "°C".to_string(),
            mass_unit: "g".to_string(),
            volume_unit: "L".to_string(),
            pressure_unit: "Pa".to_string(),
            wind_speed_unit: "m/s".to_string(),
            accumulated_precipitation_unit: "mm".to_string(),
            is_metric: true,
        }
    }

    pub fn imperial() -> Self {
        Self {
            length_unit: "mi".to_string(),
            temperature_unit: "°F".to_string(),
            mass_unit: "lb".to_string(),
            volume_unit: "gal".to_string(),
            pressure_unit: "psi".to_string(),
            wind_speed_unit: "mph".to_string(),
            accumulated_precipitation_unit: "in".to_string(),
            is_metric: false,
        }
    }
}

#[pymethods]
impl UnitSystemWrapper {
    #[getter]
    fn is_metric(&self) -> bool {
        self.is_metric
    }
}
