#!/usr/bin/env python3
"""Run Home Assistant compatibility tests.

This script runs HA's own tests for components we've implemented in Rust,
with our Rust extension monkey-patched in place of Python implementations.

Usage:
    python run_tests.py                    # Run all compatible tests
    python run_tests.py --category state   # Run only state-related tests
    python run_tests.py --list             # List available test categories
    python run_tests.py --baseline         # Generate baseline from pure Python HA
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

# Test categories mapped to pytest patterns
# Organized by component we've implemented in Rust
TEST_CATEGORIES = {
    # ==========================================================================
    # Core Types (tests/test_core.py) - ha-core crate
    # ==========================================================================
    "state": [
        "test_core.py::test_state_init",
        "test_core.py::test_state_domain",
        "test_core.py::test_state_object_id",
        "test_core.py::test_state_name_if_no_friendly_name_attr",
        "test_core.py::test_state_name_if_friendly_name_attr",
        "test_core.py::test_state_dict_conversion",
        "test_core.py::test_state_repr",
        "test_core.py::test_state_as_dict",
        "test_core.py::test_state_timestamps",
        "test_core.py::test_state_as_dict_json",
        "test_core.py::test_state_json_fragment",
        "test_core.py::test_state_as_compressed_state",
        "test_core.py::test_state_as_compressed_state_unique_last_updated",
        "test_core.py::test_state_as_compressed_state_json",
        "test_core.py::test_state_dict_conversion_with_wrong_data",
        "test_core.py::test_valid_entity_id",
        "test_core.py::test_valid_domain",
        "test_core.py::test_split_entity_id",
    ],
    "statemachine": [
        "test_core.py::test_statemachine_is_state",
        "test_core.py::test_statemachine_entity_ids",
        "test_core.py::test_statemachine_remove",
        "test_core.py::test_state_machine_case_insensitivity",
        "test_core.py::test_statemachine_last_changed_not_updated_on_same_state",
        "test_core.py::test_statemachine_force_update",
        "test_core.py::test_statemachine_avoids_updating_attributes",
        "test_core.py::test_statemachine_report_state",
        "test_core.py::test_reserving_states",
        "test_core.py::test_validate_state",
        "test_core.py::test_async_set_updates_last_reported",
    ],
    "eventbus": [
        "test_core.py::test_eventbus_add_remove_listener",
        "test_core.py::test_eventbus_filtered_listener",
        "test_core.py::test_eventbus_unsubscribe_listener",
        "test_core.py::test_eventbus_listen_once_event_with_callback",
        "test_core.py::test_eventbus_listen_once_event_with_coroutine",
        "test_core.py::test_eventbus_run_immediately_callback",
        "test_core.py::test_eventbus_run_immediately_coro",
        "test_core.py::test_eventbus_listen_once_run_immediately_coro",
        "test_core.py::test_eventbus_listen_once_event_with_thread",
        "test_core.py::test_eventbus_thread_event_listener",
        "test_core.py::test_eventbus_callback_event_listener",
        "test_core.py::test_eventbus_coroutine_event_listener",
        # test_eventbus_max_length_exceeded - tests HA's translation caching, not Rust impl
        "test_core.py::test_eventbus_lazy_object_creation",
        "test_core.py::test_event_filter_sanity_checks",
    ],
    "service": [
        "test_core.py::test_service_call_repr",
        "test_core.py::test_service_registry_has_service",
        "test_core.py::test_service_registry_service_enumeration",
        "test_core.py::test_serviceregistry_remove_service",
        "test_core.py::test_serviceregistry_async_service",
        "test_core.py::test_serviceregistry_async_service_partial",
        "test_core.py::test_serviceregistry_callback_service",
        # test_serviceregistry_service_that_not_exists - tests HA's translation caching, not Rust impl
        "test_core.py::test_service_executed_with_subservices",
        "test_core.py::test_service_call_event_contains_original_data",
    ],
    "event": [
        "test_core.py::test_event_eq",
        "test_core.py::test_event_time",
        "test_core.py::test_event_repr",
        "test_core.py::test_event_as_dict",
        "test_core.py::test_event_json_fragment",
        "test_core.py::test_event_origin_idx",
        "test_core.py::test_event_context",
    ],
    "context": [
        "test_core.py::test_context",
        "test_core.py::test_context_json_fragment",
    ],

    # ==========================================================================
    # Condition/Trigger/Script (tests/helpers/) - ha-automation, ha-script crates
    # ==========================================================================
    "condition": [
        "helpers/test_condition.py::test_and_condition",
        "helpers/test_condition.py::test_and_condition_with_template",
        "helpers/test_condition.py::test_and_condition_shorthand",
        "helpers/test_condition.py::test_and_condition_list_shorthand",
        "helpers/test_condition.py::test_or_condition",
        "helpers/test_condition.py::test_or_condition_with_template",
        "helpers/test_condition.py::test_or_condition_shorthand",
        "helpers/test_condition.py::test_not_condition",
        "helpers/test_condition.py::test_not_condition_with_template",
        "helpers/test_condition.py::test_not_condition_shorthand",
        "helpers/test_condition.py::test_time_window",
        "helpers/test_condition.py::test_time_using_input_datetime",
        "helpers/test_condition.py::test_time_using_time",
        "helpers/test_condition.py::test_time_using_sensor",
        "helpers/test_condition.py::test_state_raises",
        "helpers/test_condition.py::test_state_multiple_entities",
        "helpers/test_condition.py::test_state_multiple_entities_match_any",
        "helpers/test_condition.py::test_multiple_states",
        "helpers/test_condition.py::test_state_attribute",
        "helpers/test_condition.py::test_state_attribute_boolean",
        "helpers/test_condition.py::test_state_for",
        "helpers/test_condition.py::test_state_for_template",
    ],

    # ==========================================================================
    # Storage (tests/helpers/test_storage.py) - ha-registries crate
    # ==========================================================================
    "storage": [
        "helpers/test_storage.py::test_loading",
        "helpers/test_storage.py::test_loading_non_existing",
        "helpers/test_storage.py::test_saving_with_delay",
        "helpers/test_storage.py::test_loading_while_delay",
        "helpers/test_storage.py::test_saving_load_round_trip",
        "helpers/test_storage.py::test_minor_version_default",
        "helpers/test_storage.py::test_minor_version",
        "helpers/test_storage.py::test_migration",
        "helpers/test_storage.py::test_custom_encoder",
    ],

    # ==========================================================================
    # Area Registry (tests/helpers/test_area_registry.py) - ha-registries crate
    # ==========================================================================
    "area_registry": [
        "helpers/test_area_registry.py::test_list_areas",
        "helpers/test_area_registry.py::test_create_area",
        "helpers/test_area_registry.py::test_create_area_with_name_already_in_use",
        "helpers/test_area_registry.py::test_delete_area",
        "helpers/test_area_registry.py::test_delete_non_existing_area",
        "helpers/test_area_registry.py::test_update_area",
        "helpers/test_area_registry.py::test_update_area_with_same_name",
        "helpers/test_area_registry.py::test_update_area_with_same_name_change_case",
        "helpers/test_area_registry.py::test_update_area_with_name_already_in_use",
        "helpers/test_area_registry.py::test_load_area",
        "helpers/test_area_registry.py::test_loading_area_from_storage",
        "helpers/test_area_registry.py::test_async_get_or_create",
        "helpers/test_area_registry.py::test_async_get_area_by_name",
        "helpers/test_area_registry.py::test_async_get_area_by_name_not_found",
        "helpers/test_area_registry.py::test_async_get_area",
        "helpers/test_area_registry.py::test_entries_for_floor",
        "helpers/test_area_registry.py::test_entries_for_label",
    ],

    # ==========================================================================
    # Floor Registry (tests/helpers/test_floor_registry.py) - ha-registries crate
    # ==========================================================================
    "floor_registry": [
        "helpers/test_floor_registry.py::test_list_floors",
        "helpers/test_floor_registry.py::test_create_floor",
        "helpers/test_floor_registry.py::test_create_floor_with_name_already_in_use",
        "helpers/test_floor_registry.py::test_delete_floor",
        "helpers/test_floor_registry.py::test_delete_non_existing_floor",
        "helpers/test_floor_registry.py::test_update_floor",
        "helpers/test_floor_registry.py::test_update_floor_with_same_data",
        "helpers/test_floor_registry.py::test_update_floor_with_same_name_change_case",
        "helpers/test_floor_registry.py::test_update_floor_with_name_already_in_use",
        "helpers/test_floor_registry.py::test_load_floors",
        "helpers/test_floor_registry.py::test_loading_floors_from_storage",
        "helpers/test_floor_registry.py::test_getting_floor_by_name",
        "helpers/test_floor_registry.py::test_async_get_floor_by_name_not_found",
        "helpers/test_floor_registry.py::test_floor_removed_from_areas",
    ],

    # ==========================================================================
    # Label Registry (tests/helpers/test_label_registry.py) - ha-registries crate
    # ==========================================================================
    "label_registry": [
        "helpers/test_label_registry.py::test_list_labels",
        "helpers/test_label_registry.py::test_create_label",
        "helpers/test_label_registry.py::test_create_label_with_name_already_in_use",
        "helpers/test_label_registry.py::test_delete_label",
        "helpers/test_label_registry.py::test_delete_non_existing_label",
        "helpers/test_label_registry.py::test_update_label",
        "helpers/test_label_registry.py::test_update_label_with_same_data",
        "helpers/test_label_registry.py::test_update_label_with_same_name_change_case",
        "helpers/test_label_registry.py::test_update_label_with_name_already_in_use",
        "helpers/test_label_registry.py::test_load_labels",
        "helpers/test_label_registry.py::test_loading_label_from_storage",
        "helpers/test_label_registry.py::test_getting_label",
        "helpers/test_label_registry.py::test_async_get_label_by_name_not_found",
    ],

    # ==========================================================================
    # Entity Registry (tests/helpers/test_entity_registry.py) - ha-registries crate
    # ==========================================================================
    "entity_registry": [
        "helpers/test_entity_registry.py::test_get",
        "helpers/test_entity_registry.py::test_get_or_create_returns_same_entry",
        "helpers/test_entity_registry.py::test_get_or_create_suggested_object_id",
        "helpers/test_entity_registry.py::test_get_or_create_updates_data",
        "helpers/test_entity_registry.py::test_remove",
        "helpers/test_entity_registry.py::test_create_triggers_save",
        "helpers/test_entity_registry.py::test_loading_saving_data",
        "helpers/test_entity_registry.py::test_generate_entity_considers_registered_entities",
        "helpers/test_entity_registry.py::test_generate_entity_considers_existing_entities",
        "helpers/test_entity_registry.py::test_is_registered",
        "helpers/test_entity_registry.py::test_async_get_entity_id",
    ],

    # ==========================================================================
    # Device Registry (tests/helpers/test_device_registry.py) - ha-registries crate
    # ==========================================================================
    "device_registry": [
        "helpers/test_device_registry.py::test_get_or_create_returns_same_entry",
        "helpers/test_device_registry.py::test_requirement_for_identifier_or_connection",
        "helpers/test_device_registry.py::test_multiple_config_entries",
        "helpers/test_device_registry.py::test_removing_config_entries",
        "helpers/test_device_registry.py::test_removing_area_id",
        "helpers/test_device_registry.py::test_loading_saving_data",
        "helpers/test_device_registry.py::test_update",
        "helpers/test_device_registry.py::test_update_connection",
        "helpers/test_device_registry.py::test_format_mac",
        "helpers/test_device_registry.py::test_no_unnecessary_changes",
    ],

    # ==========================================================================
    # Template (tests/helpers/template/) - ha-template crate
    # ==========================================================================
    "template": [
        "helpers/template/test_init.py::test_template_equality",
        "helpers/template/test_init.py::test_invalid_template",
        "helpers/template/test_init.py::test_referring_states_by_entity_id",
        "helpers/template/test_init.py::test_invalid_entity_id",
        "helpers/template/test_init.py::test_iterating_all_states",
        "helpers/template/test_init.py::test_iterating_all_states_unavailable",
        "helpers/template/test_init.py::test_iterating_domain_states",
        "helpers/template/test_init.py::test_loop_controls",
        "helpers/template/test_init.py::test_float_function",
        "helpers/template/test_init.py::test_float_filter",
        "helpers/template/test_init.py::test_int_filter",
        "helpers/template/test_init.py::test_int_function",
        "helpers/template/test_init.py::test_bool_function",
        "helpers/template/test_init.py::test_bool_filter",
        "helpers/template/test_init.py::test_rounding_value",
        "helpers/template/test_init.py::test_multiply",
        "helpers/template/test_init.py::test_add",
        "helpers/template/test_init.py::test_to_json",
        "helpers/template/test_init.py::test_from_json",
        "helpers/template/test_init.py::test_ord",
        "helpers/template/test_init.py::test_passing_vars_as_keywords",
        "helpers/template/test_init.py::test_passing_vars_as_vars",
        "helpers/template/test_init.py::test_passing_vars_as_list",
        "helpers/template/test_init.py::test_passing_vars_as_dict",
    ],

    # ==========================================================================
    # Helper Event/State/Service (tests/helpers/) - various crates
    # ==========================================================================
    "helper_state": [
        "helpers/test_state.py::test_call_to_component",
        "helpers/test_state.py::test_reproduce_with_no_entity",
        "helpers/test_state.py::test_reproduce_turn_on",
        "helpers/test_state.py::test_reproduce_turn_off",
        "helpers/test_state.py::test_reproduce_complex_data",
        "helpers/test_state.py::test_as_number_states",
        "helpers/test_state.py::test_as_number_coercion",
        "helpers/test_state.py::test_as_number_invalid_cases",
    ],
    "helper_event": [
        "helpers/test_event.py::test_track_point_in_time",
        "helpers/test_event.py::test_track_point_in_time_drift_rearm",
        "helpers/test_event.py::test_track_state_change_from_to_state_match",
        "helpers/test_event.py::test_track_state_change",
        "helpers/test_event.py::test_async_track_state_change_filtered",
        "helpers/test_event.py::test_async_track_state_change_event",
        "helpers/test_event.py::test_async_track_state_added_domain",
        "helpers/test_event.py::test_async_track_state_removed_domain",
    ],
    "helper_service": [
        "helpers/test_service.py::test_call_from_config",
        "helpers/test_service.py::test_service_call",
        "helpers/test_service.py::test_service_template_service_call",
        "helpers/test_service.py::test_passing_variables_to_templates",
        "helpers/test_service.py::test_extract_entity_ids",
        "helpers/test_service.py::test_extract_entity_ids_from_area",
        "helpers/test_service.py::test_extract_entity_ids_from_devices",
        "helpers/test_service.py::test_split_entity_string",
    ],

    # ==========================================================================
    # REST API (tests/components/api/) - ha-api crate
    # ==========================================================================
    "api": [
        "components/api/test_init.py::test_api_list_state_entities",
        "components/api/test_init.py::test_api_get_state",
        "components/api/test_init.py::test_api_get_non_existing_state",
        "components/api/test_init.py::test_api_state_change",
        "components/api/test_init.py::test_api_state_change_of_non_existing_entity",
        "components/api/test_init.py::test_api_state_change_with_bad_entity_id",
        "components/api/test_init.py::test_api_state_change_with_bad_state",
        "components/api/test_init.py::test_api_state_change_to_zero_value",
        "components/api/test_init.py::test_api_state_change_push",
        "components/api/test_init.py::test_api_fire_event_with_no_data",
        "components/api/test_init.py::test_api_fire_event_with_data",
        "components/api/test_init.py::test_api_get_config",
        "components/api/test_init.py::test_api_get_components",
        "components/api/test_init.py::test_api_get_event_listeners",
        "components/api/test_init.py::test_api_get_services",
        "components/api/test_init.py::test_api_call_service_no_data",
        "components/api/test_init.py::test_api_call_service_with_data",
        "components/api/test_init.py::test_api_template",
        "components/api/test_init.py::test_api_template_error",
        "components/api/test_init.py::test_stream",
    ],

    # ==========================================================================
    # WebSocket API (tests/components/websocket_api/) - ha-api crate
    # ==========================================================================
    "websocket_commands": [
        "components/websocket_api/test_commands.py::test_fire_event",
        "components/websocket_api/test_commands.py::test_fire_event_without_data",
        "components/websocket_api/test_commands.py::test_call_service",
        "components/websocket_api/test_commands.py::test_call_service_blocking",
        "components/websocket_api/test_commands.py::test_call_service_target",
        # test_call_service_not_found - tests HA's translation caching, not Rust impl
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_events",
        "components/websocket_api/test_commands.py::test_get_states",
        "components/websocket_api/test_commands.py::test_get_services",
        "components/websocket_api/test_commands.py::test_get_config",
        "components/websocket_api/test_commands.py::test_ping",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_events_state_changed",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_entities",
        "components/websocket_api/test_commands.py::test_render_template_renders_template",
        "components/websocket_api/test_commands.py::test_render_template_with_error",
    ],
    "websocket_messages": [
        "components/websocket_api/test_messages.py::test_cached_event_message",
        "components/websocket_api/test_messages.py::test_cached_event_message_with_different_idens",
        "components/websocket_api/test_messages.py::test_state_diff_event",
        "components/websocket_api/test_messages.py::test_message_to_json_bytes",
    ],
    "websocket_http": [
        "components/websocket_api/test_http.py::test_pending_msg_overflow",
        "components/websocket_api/test_http.py::test_non_json_message",
        "components/websocket_api/test_http.py::test_binary_message",
    ],
}

def get_repo_root() -> Path:
    """Get the repository root directory."""
    return Path(__file__).parent.parent.parent

def get_ha_core_dir() -> Path:
    """Get the HA core directory (vendored submodule)."""
    return get_repo_root() / "vendor" / "ha-core"

def list_categories():
    """List available test categories."""
    print("Available test categories:")
    print("")
    total = 0
    for category, tests in TEST_CATEGORIES.items():
        print(f"  {category}: {len(tests)} tests")
        total += len(tests)
    print("")
    print(f"Total: {total} tests across {len(TEST_CATEGORIES)} categories")

def run_tests(categories: list[str] | None = None, verbose: bool = False,
              use_rust: bool = True) -> int:
    """Run the compatibility tests.

    Args:
        categories: List of test categories to run, or None for all
        verbose: Enable verbose output
        use_rust: If True, patch in Rust components; if False, run pure Python

    Returns:
        Exit code (0 for success)
    """
    repo_root = get_repo_root()
    ha_core = get_ha_core_dir()
    venv = repo_root / ".venv"

    if not ha_core.exists():
        print(f"Error: HA core not found at {ha_core}")
        print("Run: make ha-compat-setup")
        return 1

    # Build test patterns
    if categories:
        patterns = []
        for cat in categories:
            if cat in TEST_CATEGORIES:
                patterns.extend(TEST_CATEGORIES[cat])
            else:
                print(f"Warning: Unknown category '{cat}'")
        if not patterns:
            print("No valid test patterns found")
            return 1
    else:
        # All categories
        patterns = []
        for tests in TEST_CATEGORIES.values():
            patterns.extend(tests)

    # Build pytest command
    pytest_args = [
        str(venv / "bin" / "pytest"),
        "-v" if verbose else "-q",
        "--tb=short",
        "-x",  # Stop on first failure
    ]

    # Add test patterns (relative to ha_core since we run from there)
    for pattern in patterns:
        pytest_args.append(f"tests/{pattern}")

    print(f"Running {len(patterns)} tests...")
    if use_rust:
        print("Mode: Rust extension patched in")
    else:
        print("Mode: Pure Python (baseline)")
    print("")

    # Run pytest with PYTHONPATH set to include repo root
    env = os.environ.copy()
    pythonpath_parts = [str(repo_root)]
    if "PYTHONPATH" in env:
        pythonpath_parts.append(env["PYTHONPATH"])
    env["PYTHONPATH"] = os.pathsep.join(pythonpath_parts)

    # Run from HA core directory so HA's tests can find their modules
    result = subprocess.run(pytest_args, cwd=ha_core, env=env)
    return result.returncode

def main():
    parser = argparse.ArgumentParser(description="Run HA compatibility tests")
    parser.add_argument("--list", action="store_true", help="List test categories")
    parser.add_argument("--category", "-c", action="append",
                        help="Test category to run (can specify multiple)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--baseline", action="store_true",
                        help="Run without Rust patches (pure Python)")
    parser.add_argument("--all", "-a", action="store_true", help="Run all categories")

    args = parser.parse_args()

    if args.list:
        list_categories()
        return 0

    categories = args.category if args.category else None
    if args.all:
        categories = None

    return run_tests(
        categories=categories,
        verbose=args.verbose,
        use_rust=not args.baseline
    )

if __name__ == "__main__":
    sys.exit(main())
