"""Config entries shim.

This module provides a bridge to Rust config entry implementations while
maintaining API compatibility with native Home Assistant.

Rust-backed types:
- ConfigEntry: Entry data wrapper (PyConfigEntry from ha_core_rs)
- ConfigEntries: Entry manager (PyConfigEntries from ha_core_rs)
- ConfigEntryState: Entry lifecycle state enum (PyConfigEntryState from ha_core_rs)
- SOURCE_* constants

Native fallbacks (not yet ported to Rust):
- ConfigFlow, OptionsFlow, and other flow classes
- Exception classes (ConfigEntryNotReady, ConfigEntryError, ConfigEntryAuthFailed)
- Utility classes and types
"""

from homeassistant._native_loader import load_native_module

# Load native HA config_entries module for fallback
_native = load_native_module("homeassistant.config_entries")

# Try to import Rust implementations
try:
    from ha_core_rs import (
        ConfigEntry,
        ConfigEntries,
        ConfigEntryState,
        InvalidStateTransition,
    )
    from ha_core_rs.config_entries import (
        SOURCE_USER,
        SOURCE_IMPORT,
        SOURCE_DISCOVERY,
        SOURCE_DHCP,
        SOURCE_SSDP,
        SOURCE_ZEROCONF,
        SOURCE_BLUETOOTH,
        SOURCE_MQTT,
        SOURCE_HASSIO,
        SOURCE_HOMEKIT,
        SOURCE_IGNORE,
        SOURCE_REAUTH,
        SOURCE_RECONFIGURE,
        SOURCE_SYSTEM,
        SOURCE_INTEGRATION_DISCOVERY,
        SOURCE_USB,
        SOURCE_HARDWARE,
        SOURCE_ESPHOME,
    )
    _RUST_AVAILABLE = True
except ImportError:
    # Fall back to native Python HA if Rust module not available
    _RUST_AVAILABLE = False
    ConfigEntry = _native.ConfigEntry
    ConfigEntries = _native.ConfigEntries
    ConfigEntryState = _native.ConfigEntryState
    InvalidStateTransition = Exception  # Placeholder
    SOURCE_USER = _native.SOURCE_USER
    SOURCE_IMPORT = _native.SOURCE_IMPORT
    SOURCE_DISCOVERY = _native.SOURCE_DISCOVERY
    SOURCE_DHCP = _native.SOURCE_DHCP
    SOURCE_SSDP = _native.SOURCE_SSDP
    SOURCE_ZEROCONF = _native.SOURCE_ZEROCONF
    SOURCE_BLUETOOTH = _native.SOURCE_BLUETOOTH
    SOURCE_MQTT = _native.SOURCE_MQTT
    SOURCE_HASSIO = _native.SOURCE_HASSIO
    SOURCE_HOMEKIT = _native.SOURCE_HOMEKIT
    SOURCE_IGNORE = _native.SOURCE_IGNORE
    SOURCE_REAUTH = _native.SOURCE_REAUTH
    SOURCE_RECONFIGURE = _native.SOURCE_RECONFIGURE
    SOURCE_SYSTEM = _native.SOURCE_SYSTEM
    SOURCE_INTEGRATION_DISCOVERY = _native.SOURCE_INTEGRATION_DISCOVERY
    SOURCE_USB = _native.SOURCE_USB
    SOURCE_HARDWARE = _native.SOURCE_HARDWARE
    SOURCE_ESPHOME = _native.SOURCE_ESPHOME


# Re-export classes that are NOT yet ported to Rust (flow classes, exceptions, etc.)
# These are used by integrations and must be available from this module

# Exception classes (used by integrations to signal setup outcomes)
ConfigEntryError = _native.ConfigEntryError
ConfigEntryNotReady = _native.ConfigEntryNotReady
ConfigEntryAuthFailed = _native.ConfigEntryAuthFailed

# Flow classes (config flow system - complex, not yet ported)
ConfigFlow = _native.ConfigFlow
OptionsFlow = _native.OptionsFlow
OptionsFlowWithConfigEntry = _native.OptionsFlowWithConfigEntry
OptionsFlowWithReload = _native.OptionsFlowWithReload
ConfigEntryBaseFlow = _native.ConfigEntryBaseFlow
ConfigEntriesFlowManager = _native.ConfigEntriesFlowManager
OptionsFlowManager = _native.OptionsFlowManager
ConfigSubentryFlow = _native.ConfigSubentryFlow
ConfigSubentryFlowManager = _native.ConfigSubentryFlowManager

# Flow results and context
ConfigFlowResult = _native.ConfigFlowResult
ConfigFlowContext = _native.ConfigFlowContext
FlowResult = _native.FlowResult
FlowContext = _native.FlowContext
SubentryFlowContext = _native.SubentryFlowContext
SubentryFlowResult = _native.SubentryFlowResult

# Utility types
ConfigSubentry = _native.ConfigSubentry
ConfigSubentryData = _native.ConfigSubentryData
ConfigSubentryDataWithId = _native.ConfigSubentryDataWithId
DiscoveryKey = _native.DiscoveryKey
ConfigEntryChange = _native.ConfigEntryChange
ConfigEntryDisabler = _native.ConfigEntryDisabler
ConfigEntryStore = _native.ConfigEntryStore
ConfigEntryItems = _native.ConfigEntryItems

# Errors
UnknownEntry = _native.UnknownEntry
UnknownSubEntry = _native.UnknownSubEntry
OperationNotAllowed = _native.OperationNotAllowed
FlowCancelledError = _native.FlowCancelledError
ConfigError = _native.ConfigError

# Other types from native that integrations may use
EntityRegistryDisabledHandler = _native.EntityRegistryDisabledHandler
SetupPhases = _native.SetupPhases

# Constants re-exported for convenience
STORAGE_KEY = getattr(_native, "STORAGE_KEY", "core.config_entries")
STORAGE_VERSION = getattr(_native, "STORAGE_VERSION", 1)

# Registry for config flow handlers (maps domain to ConfigFlow class)
HANDLERS = _native.HANDLERS

# Build __all__ list with all exported names
__all__ = [
    # Rust-backed types
    "ConfigEntry",
    "ConfigEntries",
    "ConfigEntryState",
    "InvalidStateTransition",
    # SOURCE_* constants
    "SOURCE_USER",
    "SOURCE_IMPORT",
    "SOURCE_DISCOVERY",
    "SOURCE_DHCP",
    "SOURCE_SSDP",
    "SOURCE_ZEROCONF",
    "SOURCE_BLUETOOTH",
    "SOURCE_MQTT",
    "SOURCE_HASSIO",
    "SOURCE_HOMEKIT",
    "SOURCE_IGNORE",
    "SOURCE_REAUTH",
    "SOURCE_RECONFIGURE",
    "SOURCE_SYSTEM",
    "SOURCE_INTEGRATION_DISCOVERY",
    "SOURCE_USB",
    "SOURCE_HARDWARE",
    "SOURCE_ESPHOME",
    # Exception classes
    "ConfigEntryError",
    "ConfigEntryNotReady",
    "ConfigEntryAuthFailed",
    # Flow classes
    "ConfigFlow",
    "OptionsFlow",
    "OptionsFlowWithConfigEntry",
    "OptionsFlowWithReload",
    "ConfigEntryBaseFlow",
    "ConfigEntriesFlowManager",
    "OptionsFlowManager",
    "ConfigSubentryFlow",
    "ConfigSubentryFlowManager",
    # Flow results
    "ConfigFlowResult",
    "ConfigFlowContext",
    "FlowResult",
    "FlowContext",
    "SubentryFlowContext",
    "SubentryFlowResult",
    # Utility types
    "ConfigSubentry",
    "ConfigSubentryData",
    "ConfigSubentryDataWithId",
    "DiscoveryKey",
    "ConfigEntryChange",
    "ConfigEntryDisabler",
    "ConfigEntryStore",
    "ConfigEntryItems",
    # Errors
    "UnknownEntry",
    "UnknownSubEntry",
    "OperationNotAllowed",
    "FlowCancelledError",
    "ConfigError",
    # Other
    "EntityRegistryDisabledHandler",
    "SetupPhases",
    "STORAGE_KEY",
    "STORAGE_VERSION",
    "HANDLERS",
]

# For debugging: indicate which implementation is being used
def _is_rust_backed() -> bool:
    """Return True if using Rust implementations."""
    return _RUST_AVAILABLE
