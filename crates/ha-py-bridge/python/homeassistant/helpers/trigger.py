"""Merge native Home Assistant trigger with Rust implementations.

This shim:
1. Loads the full native trigger module (platform registration, etc.)
2. Imports Rust classes from ha_core_rs.trigger
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native trigger module
_native = load_native_module("homeassistant.helpers.trigger")

# Re-export everything from native (including private names needed by tests)
_public_names = []
for _name in dir(_native):
    # Skip dunder methods and internal loader attributes
    if _name.startswith("__") and _name.endswith("__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
try:
    from ha_core_rs.trigger import TriggerEvaluator, TriggerData, TriggerEvalContext

    globals()["TriggerEvaluator"] = TriggerEvaluator
    globals()["TriggerData"] = TriggerData
    globals()["TriggerEvalContext"] = TriggerEvalContext
    if "TriggerEvaluator" not in _public_names:
        _public_names.append("TriggerEvaluator")
    if "TriggerData" not in _public_names:
        _public_names.append("TriggerData")
    if "TriggerEvalContext" not in _public_names:
        _public_names.append("TriggerEvalContext")
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
