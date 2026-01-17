//! Home Assistant Registries
//!
//! This crate provides persistent registries for tracking:
//! - Entities (EntityRegistry)
//! - Devices (DeviceRegistry)
//! - Areas (AreaRegistry)
//! - Floors (FloorRegistry)
//! - Labels (LabelRegistry)
//!
//! All registries use JSON persistence in the `.storage/` directory
//! with versioning for migrations.

pub mod storage;

pub mod area_registry;
pub mod device_registry;
pub mod entity_registry;
pub mod floor_registry;
pub mod label_registry;

// Re-export main types
pub use storage::{Storable, Storage, StorageError, StorageFile, StorageResult};

pub use entity_registry::{
    DisabledBy, EntityCategory, EntityEntry, EntityRegistry, EntityRegistryData, HiddenBy,
};

pub use device_registry::{
    DeviceConnection, DeviceEntry, DeviceEntryType, DeviceIdentifier, DeviceRegistry,
    DeviceRegistryData,
};

pub use area_registry::{AreaEntry, AreaRegistry, AreaRegistryData};

pub use floor_registry::{FloorEntry, FloorRegistry, FloorRegistryData};

pub use label_registry::{LabelEntry, LabelRegistry, LabelRegistryData};

use std::sync::Arc;

/// All registries bundled together
pub struct Registries {
    pub storage: Arc<Storage>,
    pub entities: EntityRegistry,
    pub devices: DeviceRegistry,
    pub areas: AreaRegistry,
    pub floors: FloorRegistry,
    pub labels: LabelRegistry,
}

impl Registries {
    /// Create new registries with the given config directory
    pub fn new(config_dir: impl AsRef<std::path::Path>) -> Self {
        let storage = Arc::new(Storage::new(config_dir));

        Self {
            entities: EntityRegistry::new(storage.clone()),
            devices: DeviceRegistry::new(storage.clone()),
            areas: AreaRegistry::new(storage.clone()),
            floors: FloorRegistry::new(storage.clone()),
            labels: LabelRegistry::new(storage.clone()),
            storage,
        }
    }

    /// Load all registries from storage
    pub async fn load_all(&self) -> StorageResult<()> {
        self.entities.load().await?;
        self.devices.load().await?;
        self.areas.load().await?;
        self.floors.load().await?;
        self.labels.load().await?;
        Ok(())
    }

    /// Save all registries to storage
    pub async fn save_all(&self) -> StorageResult<()> {
        self.entities.save().await?;
        self.devices.save().await?;
        self.areas.save().await?;
        self.floors.save().await?;
        self.labels.save().await?;
        Ok(())
    }
}

// Unit tests removed - covered by HA native tests via `make ha-compat-test`
// See tests/ha_compat/ for comprehensive Registries testing through Python bindings
