"""Re-export all constants from native Home Assistant.

Constants are pure values with no logic, safe to re-export directly.
"""

from homeassistant._native_loader import load_native_module

# Load native HA const module
_native = load_native_module("homeassistant.const")

# Re-export everything including special dunder attributes like __version__
_public_names = []
for _name in dir(_native):
    # Skip private names, but include __version__ and similar public dunders
    if _name.startswith("_") and not _name.startswith("__"):
        continue
    # Skip internal dunder attributes
    if _name in ("__builtins__", "__cached__", "__doc__", "__file__",
                 "__loader__", "__name__", "__package__", "__spec__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

__all__ = _public_names
