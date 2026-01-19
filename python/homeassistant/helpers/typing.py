"""Re-export typing helpers from native Home Assistant.

Type aliases have no runtime logic, safe to re-export directly.
"""

from homeassistant._native_loader import load_native_module

# Load native HA helpers.typing module
_native = load_native_module("homeassistant.helpers.typing")

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
