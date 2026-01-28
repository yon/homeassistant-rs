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
            None, // No subentry tracking in standalone mode
            Some(name),
            None, // Use current time
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

        Ok(entry.id.clone())
    }

    /// Register an entity and return its entry info with resolved entity_id
    ///
    /// This method looks up the entity_id from the registry by unique_id.
    /// If an entry exists with the same (platform, unique_id), uses that entity_id.
    /// Otherwise generates a new entity_id.
    ///
    /// # Arguments
    /// * `domain` - Entity domain (e.g., "sensor", "light")
    /// * `platform` - Integration platform name (e.g., "airthings", "hue")
    /// * `unique_id` - Unique identifier for the entity
    /// * `suggested_object_id` - Suggested object_id for new entities (e.g., "living_room")
    /// * `config_entry_id` - The config entry that owns this entity
    /// * `device_id` - Optional device ID to link this entity to
    /// * `name` - Optional entity name
    /// * `original_device_class` - Optional device class (e.g., "temperature", "humidity")
    #[pyo3(signature = (domain, platform, unique_id, suggested_object_id=None, config_entry_id=None, device_id=None, name=None, original_device_class=None))]
    fn register_entity(
        &self,
        py: Python<'_>,
        domain: &str,
        platform: &str,
        unique_id: &str,
        suggested_object_id: Option<&str>,
        config_entry_id: Option<&str>,
        device_id: Option<&str>,
        name: Option<&str>,
        original_device_class: Option<&str>,
    ) -> PyResult<PyObject> {
        // First, check if entity with this (platform, unique_id) already exists
        let existing = self
            .registries
            .entities
            .get_by_platform_unique_id(platform, unique_id);

        // Determine the entity_id to use
        let entity_id = if let Some(ref existing_entry) = existing {
            // Entity exists - use its current entity_id
            tracing::debug!(
                entity_id = %existing_entry.entity_id,
                unique_id = %unique_id,
                platform = %platform,
                "Found existing entity_id in registry"
            );
            existing_entry.entity_id.clone()
        } else {
            // New entity - generate entity_id
            let object_id = suggested_object_id
                .map(String::from)
                .unwrap_or_else(|| format!("{}_{}", platform, unique_id));
            let generated = self
                .registries
                .entities
                .generate_entity_id(domain, &object_id, None, None);
            tracing::debug!(
                entity_id = %generated,
                unique_id = %unique_id,
                platform = %platform,
                "Generated new entity_id"
            );
            generated
        };

        // Get or create the entry with the resolved entity_id
        let mut entry = self.registries.entities.get_or_create(
            platform,
            &entity_id,
            Some(unique_id),
            config_entry_id,
            device_id,
        );

        // Update name and device_class if provided
        if name.is_some() || original_device_class.is_some() {
            entry = self
                .registries
                .entities
                .update(&entry.entity_id, |e| {
                    if let Some(n) = name {
                        e.name = Some(n.to_string());
                    }
                    if let Some(dc) = original_device_class {
                        e.original_device_class = Some(dc.to_string());
                    }
                })
                .expect("Entity should exist after get_or_create");
        }

        tracing::info!(
            entity_id = %entry.entity_id,
            unique_id = %unique_id,
            platform = %platform,
            device_id = ?device_id,
            "Registered entity in Rust registry"
        );

        // Return entry info as a dict - entity_id is the key field for Python
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

    /// Look up entity_id by platform and unique_id
    ///
    /// Returns the entity_id if found, None otherwise.
    /// Used to map unique_id to human-readable entity_id from the registry.
    #[pyo3(signature = (domain, platform, unique_id))]
    fn get_entity_id(&self, domain: &str, platform: &str, unique_id: &str) -> Option<String> {
        self.registries
            .entities
            .get_by_platform_unique_id(platform, unique_id)
            .filter(|e| e.domain() == domain)
            .map(|e| e.entity_id.clone())
    }
}
