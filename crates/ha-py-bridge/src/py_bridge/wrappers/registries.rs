//! RegistriesWrapper - wraps Rust Registries for device/entity registration

use ha_registries::{DeviceConnection, DeviceIdentifier, Registries};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::sync::Arc;

/// Python wrapper for the Rust Registries
///
/// Provides methods to register devices and entities from Python integrations.
#[pyclass(name = "RegistriesWrapper")]
pub struct RegistriesWrapper {
    registries: Arc<Registries>,
}

impl RegistriesWrapper {
    pub fn new(registries: Arc<Registries>) -> Self {
        Self { registries }
    }
}

#[pymethods]
impl RegistriesWrapper {
    /// Register a device and return its device_id
    ///
    /// # Arguments
    /// * `config_entry_id` - The config entry that owns this device
    /// * `identifiers` - List of (domain, id) tuples to identify the device
    /// * `connections` - List of (connection_type, id) tuples (e.g., MAC addresses)
    /// * `name` - Device name
    /// * `manufacturer` - Optional manufacturer name
    /// * `model` - Optional model name
    /// * `sw_version` - Optional software version
    /// * `hw_version` - Optional hardware version
    #[pyo3(signature = (config_entry_id, identifiers, connections, name, manufacturer=None, model=None, sw_version=None, hw_version=None))]
    fn register_device(
        &self,
        config_entry_id: &str,
        identifiers: &Bound<'_, PyList>,
        connections: &Bound<'_, PyList>,
        name: &str,
        manufacturer: Option<&str>,
        model: Option<&str>,
        sw_version: Option<&str>,
        hw_version: Option<&str>,
    ) -> PyResult<String> {
        // Convert identifiers from Python list of tuples to Vec<DeviceIdentifier>
        let mut device_identifiers = Vec::new();
        for item in identifiers.iter() {
            if let Ok(tuple) = item.downcast::<PyTuple>() {
                if tuple.len() >= 2 {
                    let domain: String = tuple.get_item(0)?.extract()?;
                    let id: String = tuple.get_item(1)?.extract()?;
                    device_identifiers.push(DeviceIdentifier::new(domain, id));
                }
            } else if let Ok(list) = item.downcast::<PyList>() {
                if list.len() >= 2 {
                    let domain: String = list.get_item(0)?.extract()?;
                    let id: String = list.get_item(1)?.extract()?;
                    device_identifiers.push(DeviceIdentifier::new(domain, id));
                }
            }
        }

        // Convert connections from Python list of tuples to Vec<DeviceConnection>
        let mut device_connections = Vec::new();
        for item in connections.iter() {
            if let Ok(tuple) = item.downcast::<PyTuple>() {
                if tuple.len() >= 2 {
                    let conn_type: String = tuple.get_item(0)?.extract()?;
                    let id: String = tuple.get_item(1)?.extract()?;
                    device_connections.push(DeviceConnection::new(conn_type, id));
                }
            } else if let Ok(list) = item.downcast::<PyList>() {
                if list.len() >= 2 {
                    let conn_type: String = list.get_item(0)?.extract()?;
                    let id: String = list.get_item(1)?.extract()?;
                    device_connections.push(DeviceConnection::new(conn_type, id));
                }
            }
        }

        // Register the device
        let mut entry = self.registries.devices.get_or_create(
            &device_identifiers,
            &device_connections,
            Some(config_entry_id),
            name,
        );

        // Update additional fields
        if manufacturer.is_some() || model.is_some() || sw_version.is_some() || hw_version.is_some()
        {
            if let Some(updated) = self.registries.devices.update(&entry.id, |e| {
                if let Some(m) = manufacturer {
                    e.manufacturer = Some(m.to_string());
                }
                if let Some(m) = model {
                    e.model = Some(m.to_string());
                }
                if let Some(v) = sw_version {
                    e.sw_version = Some(v.to_string());
                }
                if let Some(v) = hw_version {
                    e.hw_version = Some(v.to_string());
                }
            }) {
                entry = updated;
            }
        }

        tracing::info!(
            device_id = %entry.id,
            name = %name,
            "Registered device in Rust registry"
        );

        Ok(entry.id)
    }

    /// Register an entity and return its entry info
    ///
    /// # Arguments
    /// * `platform` - Platform name (e.g., "light", "sensor")
    /// * `entity_id` - The entity ID (e.g., "light.living_room")
    /// * `unique_id` - Optional unique identifier for the entity
    /// * `config_entry_id` - The config entry that owns this entity
    /// * `device_id` - Optional device ID to link this entity to
    /// * `name` - Optional entity name
    #[pyo3(signature = (platform, entity_id, unique_id=None, config_entry_id=None, device_id=None, name=None))]
    fn register_entity(
        &self,
        py: Python<'_>,
        platform: &str,
        entity_id: &str,
        unique_id: Option<&str>,
        config_entry_id: Option<&str>,
        device_id: Option<&str>,
        name: Option<&str>,
    ) -> PyResult<PyObject> {
        let mut entry = self.registries.entities.get_or_create(
            platform,
            entity_id,
            unique_id,
            config_entry_id,
            device_id,
        );

        // Update name if provided
        if let Some(n) = name {
            entry = self.registries.entities.update(&entry.entity_id, |e| {
                e.name = Some(n.to_string());
            });
        }

        tracing::info!(
            entity_id = %entity_id,
            platform = %platform,
            device_id = ?device_id,
            "Registered entity in Rust registry"
        );

        // Return entry info as a dict
        let dict = PyDict::new_bound(py);
        dict.set_item("entity_id", &entry.entity_id)?;
        dict.set_item("unique_id", &entry.unique_id)?;
        dict.set_item("platform", &entry.platform)?;
        dict.set_item("config_entry_id", &entry.config_entry_id)?;
        dict.set_item("device_id", &entry.device_id)?;
        dict.set_item("name", &entry.name)?;
        dict.set_item("id", &entry.id)?;

        Ok(dict.into())
    }

    /// Get device count
    fn device_count(&self) -> usize {
        self.registries.devices.len()
    }

    /// Get entity count
    fn entity_count(&self) -> usize {
        self.registries.entities.len()
    }
}
