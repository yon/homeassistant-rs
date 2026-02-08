"""Merge native Home Assistant condition with Rust implementations.

This shim:
1. Loads the full native condition module (async_from_config, etc.)
2. Imports Rust classes from ha_core_rs.condition
3. Re-exports both, with Rust classes taking precedence
"""

from homeassistant._native_loader import load_native_module

# Load native condition module
_native = load_native_module("homeassistant.helpers.condition")

# Re-export everything from native
_public_names = []
for _name in dir(_native):
    if _name.startswith("__") and _name.endswith("__"):
        continue
    _public_names.append(_name)
    globals()[_name] = getattr(_native, _name)

# Import Rust classes from ha_core_rs (they take precedence)
try:
    from ha_core_rs.condition import ConditionEvaluator, EvalContext

    globals()["ConditionEvaluator"] = ConditionEvaluator
    globals()["EvalContext"] = EvalContext
    if "ConditionEvaluator" not in _public_names:
        _public_names.append("ConditionEvaluator")
    if "EvalContext" not in _public_names:
        _public_names.append("EvalContext")
except ImportError:
    # ha_core_rs not available (e.g., in pure Python mode)
    pass

__all__ = _public_names
