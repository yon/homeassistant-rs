"""Re-export floor_registry from native Home Assistant."""

from homeassistant._native_loader import load_native_module

_native = load_native_module("homeassistant.helpers.floor_registry")

_public_names = []
for _name in dir(_native):
    if _name.startswith("_"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

__all__ = _public_names
