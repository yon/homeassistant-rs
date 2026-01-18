"""Root conftest.py to inject mock homeassistant package.

This runs before vendor/ha-core/tests/conftest.py, so we can manipulate
sys.path before the real homeassistant is imported.
"""
import sys
import os
from pathlib import Path

# Only activate mock mode when HOMEASSISTANT_MOCK environment variable is set
if os.environ.get("HOMEASSISTANT_MOCK"):
    mock_path = str(Path(__file__).parent / "python")

    # Insert mock package at the beginning of sys.path
    if mock_path not in sys.path:
        sys.path.insert(0, mock_path)

    # Remove any cached homeassistant imports
    mods_to_remove = [k for k in list(sys.modules.keys())
                      if k == "homeassistant" or k.startswith("homeassistant.")]
    for mod in mods_to_remove:
        del sys.modules[mod]

    print(f"*** MOCK MODE: Injected {mock_path} into sys.path ***", file=sys.stderr)
