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

pub mod entity_registry;
pub mod device_registry;
pub mod area_registry;
pub mod floor_registry;
pub mod label_registry;

// Re-export main types
pub use storage::{Storage, StorageError, StorageFile, StorageResult, Storable};

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_registries_bundle() {
        let temp_dir = TempDir::new().unwrap();
        let registries = Registries::new(temp_dir.path());

        // Create some data
        registries.areas.create("Living Room");
        registries.floors.create("Ground Floor", 0);
        registries.labels.create("Critical");

        let entity = registries.entities.get_or_create(
            "hue",
            "light.living_room",
            Some("unique1"),
            None,
            None,
        );

        let _device = registries.devices.get_or_create(
            &[device_registry::DeviceIdentifier::new("hue", "bridge1")],
            &[],
            None,
            "Hue Bridge",
        );

        // Update entity with device
        registries.entities.update(&entity.entity_id, |e| {
            e.device_id = Some("device1".to_string());
        });

        // Save all
        registries.save_all().await.unwrap();

        // Load into new registries
        let registries2 = Registries::new(temp_dir.path());
        registries2.load_all().await.unwrap();

        assert_eq!(registries2.entities.len(), 1);
        assert_eq!(registries2.devices.len(), 1);
        assert_eq!(registries2.areas.len(), 1);
        assert_eq!(registries2.floors.len(), 1);
        assert_eq!(registries2.labels.len(), 1);
    }
}
