"""Entity platform helpers - re-export from native HA.

AddEntitiesCallback is a type alias, safe to re-export.
EntityPlatform contains logic that may need Rust backing later.
"""

from homeassistant._native_loader import load_native_module

# Load native HA entity_platform module
_native = load_native_module("homeassistant.helpers.entity_platform")

# Internal dunder attributes that should NOT be re-exported
_SKIP_ATTRS = frozenset((
    "__builtins__", "__cached__", "__doc__", "__file__",
    "__loader__", "__name__", "__package__", "__spec__",
))

# Re-export everything except internal attributes
_exported_names = []
for _name in dir(_native):
    if _name in _SKIP_ATTRS:
        continue
    _exported_names.append(_name)
    globals()[_name] = getattr(_native, _name)

__all__ = _exported_names
