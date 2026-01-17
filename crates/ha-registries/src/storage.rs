//! Storage abstraction for JSON persistence
//!
//! Implements the Home Assistant `.storage/` directory pattern with versioning.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;
use tracing::{debug, warn};

/// Storage errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Storage file not found: {key}")]
    NotFound { key: String },

    #[error("Version mismatch for {key}: expected {expected}, found {found}")]
    VersionMismatch {
        key: String,
        expected: u32,
        found: u32,
    },

    #[error("Migration required for {key}: from {from} to {to}")]
    MigrationRequired { key: String, from: u32, to: u32 },
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage file wrapper with version tracking
///
/// JSON format:
/// ```json
/// {
///   "version": 1,
///   "minor_version": 1,
///   "key": "core.entity_registry",
///   "data": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageFile<T> {
    /// Major version - breaking changes
    pub version: u32,
    /// Minor version - migrations within major version
    pub minor_version: u32,
    /// Storage key (file identifier)
    pub key: String,
    /// The actual data
    pub data: T,
}

impl<T> StorageFile<T> {
    /// Create a new storage file
    pub fn new(key: impl Into<String>, data: T, version: u32, minor_version: u32) -> Self {
        Self {
            version,
            minor_version,
            key: key.into(),
            data,
        }
    }
}

/// Storage manager for handling `.storage/` directory
#[derive(Debug, Clone)]
pub struct Storage {
    /// Path to the `.storage/` directory
    storage_dir: PathBuf,
}

impl Storage {
    /// Create a new storage manager
    ///
    /// # Arguments
    /// * `config_dir` - Path to the Home Assistant config directory
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            storage_dir: config_dir.as_ref().join(".storage"),
        }
    }

    /// Get the storage directory path
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    /// Ensure the storage directory exists
    pub async fn ensure_dir(&self) -> StorageResult<()> {
        if !self.storage_dir.exists() {
            fs::create_dir_all(&self.storage_dir).await?;
            debug!("Created storage directory: {:?}", self.storage_dir);
        }
        Ok(())
    }

    /// Get the file path for a storage key
    pub fn file_path(&self, key: &str) -> PathBuf {
        self.storage_dir.join(key)
    }

    /// Check if a storage key exists
    pub async fn exists(&self, key: &str) -> bool {
        self.file_path(key).exists()
    }

    /// Load data from storage
    ///
    /// Returns None if the file doesn't exist.
    pub async fn load<T>(&self, key: &str) -> StorageResult<Option<StorageFile<T>>>
    where
        T: DeserializeOwned,
    {
        let path = self.file_path(key);

        if !path.exists() {
            debug!("Storage file not found: {}", key);
            return Ok(None);
        }

        let content = fs::read_to_string(&path).await?;
        let storage_file: StorageFile<T> = serde_json::from_str(&content)?;

        debug!(
            "Loaded storage file: {} (v{}.{})",
            key, storage_file.version, storage_file.minor_version
        );

        Ok(Some(storage_file))
    }

    /// Load data from storage, returning an error if not found
    pub async fn load_required<T>(&self, key: &str) -> StorageResult<StorageFile<T>>
    where
        T: DeserializeOwned,
    {
        self.load(key).await?.ok_or_else(|| StorageError::NotFound {
            key: key.to_string(),
        })
    }

    /// Save data to storage
    ///
    /// Writes atomically by first writing to a temp file, then renaming.
    pub async fn save<T>(&self, storage_file: &StorageFile<T>) -> StorageResult<()>
    where
        T: Serialize,
    {
        self.ensure_dir().await?;

        let path = self.file_path(&storage_file.key);
        let temp_path = self.file_path(&format!("{}.tmp", storage_file.key));

        // Serialize with pretty printing for readability
        let content = serde_json::to_string_pretty(storage_file)?;

        // Write to temp file first
        fs::write(&temp_path, &content).await?;

        // Atomic rename
        fs::rename(&temp_path, &path).await?;

        debug!(
            "Saved storage file: {} (v{}.{})",
            storage_file.key, storage_file.version, storage_file.minor_version
        );

        Ok(())
    }

    /// Delete a storage file
    pub async fn delete(&self, key: &str) -> StorageResult<()> {
        let path = self.file_path(key);

        if path.exists() {
            fs::remove_file(&path).await?;
            debug!("Deleted storage file: {}", key);
        }

        Ok(())
    }

    /// List all storage keys
    pub async fn list_keys(&self) -> StorageResult<Vec<String>> {
        if !self.storage_dir.exists() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::new();
        let mut entries = fs::read_dir(&self.storage_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        // Skip temp files
                        if !name.ends_with(".tmp") {
                            keys.push(name.to_string());
                        }
                    }
                }
            }
        }

        Ok(keys)
    }
}

/// Helper trait for types that can be stored
pub trait Storable: Serialize + DeserializeOwned {
    /// Storage key for this type
    const KEY: &'static str;
    /// Current major version
    const VERSION: u32;
    /// Current minor version
    const MINOR_VERSION: u32;

    /// Create a storage file wrapper
    fn to_storage_file(&self) -> StorageFile<Self>
    where
        Self: Clone,
    {
        StorageFile::new(Self::KEY, self.clone(), Self::VERSION, Self::MINOR_VERSION)
    }
}

/// Migration function type
pub type MigrationFn<T> = fn(serde_json::Value, u32) -> StorageResult<T>;

/// Load with migration support
pub async fn load_with_migration<T>(
    storage: &Storage,
    migrate: Option<MigrationFn<T>>,
) -> StorageResult<Option<T>>
where
    T: Storable,
{
    let path = storage.file_path(T::KEY);

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).await?;

    // First, parse just the version info
    #[derive(Deserialize)]
    struct VersionInfo {
        version: u32,
        minor_version: u32,
    }

    let version_info: VersionInfo = serde_json::from_str(&content)?;

    // Check if migration is needed
    if version_info.version != T::VERSION {
        if let Some(migrate_fn) = migrate {
            // Parse as raw JSON for migration
            let raw: serde_json::Value = serde_json::from_str(&content)?;
            let data = raw.get("data").cloned().unwrap_or(serde_json::Value::Null);
            return Ok(Some(migrate_fn(data, version_info.version)?));
        } else {
            return Err(StorageError::MigrationRequired {
                key: T::KEY.to_string(),
                from: version_info.version,
                to: T::VERSION,
            });
        }
    }

    // Same version, direct deserialize
    let storage_file: StorageFile<T> = serde_json::from_str(&content)?;

    if version_info.minor_version < T::MINOR_VERSION {
        warn!(
            "Storage {} has older minor version ({} < {}), may need migration",
            T::KEY,
            version_info.minor_version,
            T::MINOR_VERSION
        );
    }

    Ok(Some(storage_file.data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    impl Storable for TestData {
        const KEY: &'static str = "test.data";
        const VERSION: u32 = 1;
        const MINOR_VERSION: u32 = 1;
    }

    #[tokio::test]
    async fn test_storage_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path());

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let storage_file = StorageFile::new("test.data", data.clone(), 1, 1);

        // Save
        storage.save(&storage_file).await.unwrap();

        // Verify file exists
        assert!(storage.exists("test.data").await);

        // Load
        let loaded: StorageFile<TestData> = storage.load_required("test.data").await.unwrap();
        assert_eq!(loaded.data, data);
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.minor_version, 1);
    }

    #[tokio::test]
    async fn test_storage_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path());

        let result: Option<StorageFile<TestData>> = storage.load("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_storage_list_keys() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path());

        // Save multiple files
        for i in 0..3 {
            let data = TestData {
                name: format!("test{}", i),
                value: i,
            };
            let storage_file = StorageFile::new(format!("test.{}", i), data, 1, 1);
            storage.save(&storage_file).await.unwrap();
        }

        let keys = storage.list_keys().await.unwrap();
        assert_eq!(keys.len(), 3);
    }

    #[tokio::test]
    async fn test_storage_delete() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path());

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let storage_file = StorageFile::new("test.data", data, 1, 1);

        storage.save(&storage_file).await.unwrap();
        assert!(storage.exists("test.data").await);

        storage.delete("test.data").await.unwrap();
        assert!(!storage.exists("test.data").await);
    }

    #[tokio::test]
    async fn test_load_with_storable() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path());

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // Save using Storable trait
        storage.save(&data.to_storage_file()).await.unwrap();

        // Load with migration support
        let loaded: Option<TestData> = load_with_migration(&storage, None).await.unwrap();
        assert_eq!(loaded, Some(data));
    }
}
