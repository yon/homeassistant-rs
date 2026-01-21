"""Re-export entity_registry from native Home Assistant.

This shim ensures the full native entity_registry module is loaded,
not just the partial PyO3 module from ha_core_rs.
"""

from homeassistant._native_loader import load_native_module

# Load native entity_registry module
_native = load_native_module("homeassistant.helpers.entity_registry")

# Re-export everything
_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

__all__ = _public_names
