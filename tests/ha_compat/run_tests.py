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
import importlib.util
import json
import os
import subprocess
import sys
from pathlib import Path

# Test categories mapped to pytest patterns
# Organized by component we've implemented in Rust
TEST_CATEGORIES = {
    # ==========================================================================
    # Config Entries (tests/test_config_entries.py) - ha-config-entries crate
    # ==========================================================================
    "config_entries": [
        # Basic entry management
        "test_config_entries.py::test_entries_gets_entries",
        "test_config_entries.py::test_domains_gets_domains_uniques",
        "test_config_entries.py::test_entries_excludes_ignore_and_disabled",
        # Setup and lifecycle
        "test_config_entries.py::test_call_setup_entry",
        "test_config_entries.py::test_add_entry_calls_setup_entry",
        "test_config_entries.py::test_entry_setup_succeed",
        "test_config_entries.py::test_entry_setup_invalid_state",
        # Unload
        "test_config_entries.py::test_entry_unload",
        "test_config_entries.py::test_entry_unload_invalid_state",
        # Reload
        "test_config_entries.py::test_entry_reload_succeed",
        "test_config_entries.py::test_entry_reload_not_loaded",
        # Remove
        "test_config_entries.py::test_remove_entry",
        "test_config_entries.py::test_remove_entry_if_not_loaded",
        # Disable/Enable
        "test_config_entries.py::test_entry_disable_succeed",
        # Update
        "test_config_entries.py::test_updating_entry_data",
        "test_config_entries.py::test_updating_entry_with_and_without_changes",
        # Unique ID
        "test_config_entries.py::test_unique_id_persisted",
        "test_config_entries.py::test_unique_id_existing_entry",
        # State transitions and errors
        "test_config_entries.py::test_setup_raise_not_ready",
        "test_config_entries.py::test_setup_raise_entry_error",
        "test_config_entries.py::test_setup_raise_auth_failed",
        "test_config_entries.py::test_entry_state_change_calls_listener",
        # Storage/persistence
        "test_config_entries.py::test_saving_and_loading",
        "test_config_entries.py::test_loading_default_config",
        # Migration
        "test_config_entries.py::test_call_async_migrate_entry",
    ],

    # ==========================================================================
    # Config Entry State Transitions - tests for FSM validation
    # ==========================================================================
    "config_entries_state": [
        # Setup retry behavior
        "test_config_entries.py::test_setup_raise_not_ready",
        "test_config_entries.py::test_setup_raise_not_ready_from_exception",
        "test_config_entries.py::test_setup_retrying_during_unload",
        "test_config_entries.py::test_setup_retrying_during_unload_before_started",
        "test_config_entries.py::test_reload_during_setup_retrying_waits",
        # Unload during various states
        "test_config_entries.py::test_entry_unload",
        "test_config_entries.py::test_entry_unload_failed_to_load",
        "test_config_entries.py::test_entry_unload_invalid_state",
        # Setup state validation
        "test_config_entries.py::test_entry_setup_succeed",
        "test_config_entries.py::test_entry_setup_invalid_state",
        # Lock validation
        "test_config_entries.py::test_entry_setup_without_lock_raises",
        "test_config_entries.py::test_entry_unload_without_lock_raises",
        # State change listeners
        "test_config_entries.py::test_entry_state_change_calls_listener",
        "test_config_entries.py::test_entry_state_change_wrapped_in_on_unload",
        "test_config_entries.py::test_entry_state_change_listener_removed",
        "test_config_entries.py::test_entry_state_change_error_does_not_block_transition",
        # Error states
        "test_config_entries.py::test_setup_raise_entry_error",
        "test_config_entries.py::test_setup_raise_entry_error_from_first_coordinator_update",
    ],

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
    # Condition (tests/helpers/test_condition.py) - ha-automation crate
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
    # Trigger (tests/helpers/test_trigger.py) - ha-automation crate
    # ==========================================================================
    "trigger": [
        "helpers/test_trigger.py::test_bad_trigger_platform",
        "helpers/test_trigger.py::test_trigger_subtype",
        "helpers/test_trigger.py::test_trigger_variables",
        "helpers/test_trigger.py::test_if_disabled_trigger_not_firing",
        "helpers/test_trigger.py::test_trigger_enabled_templates",
        "helpers/test_trigger.py::test_nested_trigger_list",
        "helpers/test_trigger.py::test_trigger_enabled_template_limited",
        "helpers/test_trigger.py::test_trigger_alias",
        "helpers/test_trigger.py::test_async_initialize_triggers",
        "helpers/test_trigger.py::test_pluggable_action",
        "helpers/test_trigger.py::test_platform_multiple_triggers",
        "helpers/test_trigger.py::test_platform_migrate_trigger",
        "helpers/test_trigger.py::test_platform_backwards_compatibility_for_new_style_configs",
        "helpers/test_trigger.py::test_invalid_trigger_platform",
        "helpers/test_trigger.py::test_subscribe_triggers",
        "helpers/test_trigger.py::test_subscribe_triggers_no_triggers",
        "helpers/test_trigger.py::test_numerical_state_attribute_changed_error_handling",
    ],

    # ==========================================================================
    # Script (tests/helpers/test_script.py) - ha-script crate
    # ==========================================================================
    "script": [
        # Basic actions
        "helpers/test_script.py::test_firing_event_basic",
        "helpers/test_script.py::test_firing_event_template",
        "helpers/test_script.py::test_calling_service_basic",
        "helpers/test_script.py::test_calling_service_template",
        "helpers/test_script.py::test_data_template_with_templated_key",
        "helpers/test_script.py::test_activating_scene",
        # Delays
        "helpers/test_script.py::test_delay_basic",
        "helpers/test_script.py::test_empty_delay",
        "helpers/test_script.py::test_delay_template_ok",
        "helpers/test_script.py::test_delay_template_invalid",
        "helpers/test_script.py::test_cancel_delay",
        # Wait actions
        "helpers/test_script.py::test_wait_basic[template]",
        "helpers/test_script.py::test_wait_basic[trigger]",
        "helpers/test_script.py::test_wait_basic_times_out[template]",
        "helpers/test_script.py::test_wait_basic_times_out[trigger]",
        "helpers/test_script.py::test_wait_template_not_schedule",
        "helpers/test_script.py::test_wait_for_trigger_variables",
        # Conditions
        "helpers/test_script.py::test_condition_basic",
        "helpers/test_script.py::test_condition_validation",
        # Choose/If-Then
        "helpers/test_script.py::test_choose[1-first]",
        "helpers/test_script.py::test_choose[2-second]",
        "helpers/test_script.py::test_choose[3-default]",
        "helpers/test_script.py::test_if[1-True-then]",
        "helpers/test_script.py::test_if[2-False-else]",
        # Repeat
        "helpers/test_script.py::test_repeat_count[3]",
        "helpers/test_script.py::test_repeat_count[40]",
        "helpers/test_script.py::test_repeat_count_0",
        "helpers/test_script.py::test_repeat_for_each",
        "helpers/test_script.py::test_repeat_conditional[False-while]",
        "helpers/test_script.py::test_repeat_conditional[True-until]",
        # Parallel
        "helpers/test_script.py::test_parallel",
        # test_parallel_error - tests HA's translation caching, not Rust impl
        # Variables
        "helpers/test_script.py::test_set_variable",
        "helpers/test_script.py::test_set_redefines_variable",
        # Control flow
        "helpers/test_script.py::test_stop_action",
        "helpers/test_script.py::test_stop_action_with_error",
        "helpers/test_script.py::test_stop_no_wait[1]",
        "helpers/test_script.py::test_stop_no_wait[3]",
        # Execution modes
        "helpers/test_script.py::test_multiple_runs_no_wait",
        "helpers/test_script.py::test_multiple_runs_delay",
        "helpers/test_script.py::test_multiple_runs_wait[template]",
        "helpers/test_script.py::test_multiple_runs_wait[trigger]",
        "helpers/test_script.py::test_script_mode_single",
        "helpers/test_script.py::test_script_mode_queued",
        "helpers/test_script.py::test_script_mode_2[restart-messages0-last_events0]",
        "helpers/test_script.py::test_script_mode_2[parallel-messages1-last_events1]",
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
        # Basic CRUD operations
        "helpers/test_entity_registry.py::test_get",
        "helpers/test_entity_registry.py::test_get_or_create_returns_same_entry",
        "helpers/test_entity_registry.py::test_get_or_create_suggested_object_id",
        "helpers/test_entity_registry.py::test_get_or_create_updates_data",
        "helpers/test_entity_registry.py::test_get_or_create_suggested_object_id_conflict_register",
        "helpers/test_entity_registry.py::test_get_or_create_suggested_object_id_conflict_existing",
        "helpers/test_entity_registry.py::test_remove",
        # Note: test_create_triggers_save skipped - tests Python-internal save scheduling
        "helpers/test_entity_registry.py::test_loading_saving_data",
        "helpers/test_entity_registry.py::test_generate_entity_considers_registered_entities",
        "helpers/test_entity_registry.py::test_generate_entity_considers_existing_entities",
        "helpers/test_entity_registry.py::test_is_registered",
        "helpers/test_entity_registry.py::test_async_get_entity_id",
        # Config entry management
        "helpers/test_entity_registry.py::test_updating_config_entry_id",
        "helpers/test_entity_registry.py::test_removing_config_entry_id",
        "helpers/test_entity_registry.py::test_deleted_entity_removing_config_entry_id",
        "helpers/test_entity_registry.py::test_removing_config_subentry_id",
        "helpers/test_entity_registry.py::test_deleted_entity_removing_config_subentry_id",
        # Area management
        "helpers/test_entity_registry.py::test_removing_area_id",
        "helpers/test_entity_registry.py::test_removing_area_id_deleted_entity",
        # Entity updates
        "helpers/test_entity_registry.py::test_update_entity_unique_id",
        "helpers/test_entity_registry.py::test_update_entity_unique_id_conflict",
        "helpers/test_entity_registry.py::test_update_entity_entity_id",
        "helpers/test_entity_registry.py::test_update_entity_entity_id_without_state",
        "helpers/test_entity_registry.py::test_update_entity_entity_id_entity_id",
        "helpers/test_entity_registry.py::test_update_entity",
        "helpers/test_entity_registry.py::test_update_entity_options",
        # Disabled/hidden state
        "helpers/test_entity_registry.py::test_disabled_by",
        "helpers/test_entity_registry.py::test_disabled_by_config_entry_pref",
        "helpers/test_entity_registry.py::test_hidden_by",
        "helpers/test_entity_registry.py::test_update_entity_disabled_by",
        "helpers/test_entity_registry.py::test_update_entity_disabled_by_2",
        "helpers/test_entity_registry.py::test_disabled_entities_excluded_from_entity_list",
        # Device interaction
        "helpers/test_entity_registry.py::test_remove_device_removes_entities",
        "helpers/test_entity_registry.py::test_remove_config_entry_from_device_removes_entities",
        "helpers/test_entity_registry.py::test_remove_config_entry_from_device_removes_entities_2",
        "helpers/test_entity_registry.py::test_remove_config_subentry_from_device_removes_entities",
        "helpers/test_entity_registry.py::test_remove_config_subentry_from_device_removes_entities_2",
        "helpers/test_entity_registry.py::test_disable_device_disables_entities",
        "helpers/test_entity_registry.py::test_disable_config_entry_disables_entities",
        # Labels and categories
        "helpers/test_entity_registry.py::test_removing_labels",
        "helpers/test_entity_registry.py::test_removing_labels_deleted_entity",
        "helpers/test_entity_registry.py::test_entries_for_label",
        "helpers/test_entity_registry.py::test_removing_categories",
        "helpers/test_entity_registry.py::test_removing_categories_deleted_entity",
        "helpers/test_entity_registry.py::test_entries_for_category",
        # Validation
        "helpers/test_entity_registry.py::test_entity_max_length_exceeded",
        "helpers/test_entity_registry.py::test_resolve_entity_ids",
        "helpers/test_entity_registry.py::test_entity_registry_items",
        "helpers/test_entity_registry.py::test_config_entry_does_not_exist",
        "helpers/test_entity_registry.py::test_device_does_not_exist",
        "helpers/test_entity_registry.py::test_disabled_by_str_not_allowed",
        "helpers/test_entity_registry.py::test_entity_category_str_not_allowed",
        "helpers/test_entity_registry.py::test_hidden_by_str_not_allowed",
        "helpers/test_entity_registry.py::test_unique_id_non_hashable",
        "helpers/test_entity_registry.py::test_unique_id_non_string",
        # Restore and migration
        "helpers/test_entity_registry.py::test_restore_states",
        "helpers/test_entity_registry.py::test_restore_entity",
        "helpers/test_entity_registry.py::test_restore_entity_disabled_by",
        "helpers/test_entity_registry.py::test_restore_entity_disabled_by_2",
        "helpers/test_entity_registry.py::test_migrate_entity_to_new_platform",
        "helpers/test_entity_registry.py::test_migrate_entity_to_new_platform_error_handling",
        "helpers/test_entity_registry.py::test_async_migrate_entry_delete_self",
        "helpers/test_entity_registry.py::test_async_migrate_entry_delete_other",
        # Subentry
        "helpers/test_entity_registry.py::test_subentry",
    ],

    # ==========================================================================
    # Device Registry (tests/helpers/test_device_registry.py) - ha-registries crate
    # ==========================================================================
    "device_registry": [
        # Basic CRUD operations
        "helpers/test_device_registry.py::test_get_or_create_returns_same_entry",
        "helpers/test_device_registry.py::test_requirement_for_identifier_or_connection",
        "helpers/test_device_registry.py::test_multiple_config_entries",
        "helpers/test_device_registry.py::test_multiple_config_subentries",
        "helpers/test_device_registry.py::test_loading_from_storage",
        "helpers/test_device_registry.py::test_loading_saving_data",
        "helpers/test_device_registry.py::test_format_mac",
        "helpers/test_device_registry.py::test_no_unnecessary_changes",
        # Config entry management
        "helpers/test_device_registry.py::test_removing_config_entries",
        "helpers/test_device_registry.py::test_deleted_device_removing_config_entries",
        "helpers/test_device_registry.py::test_removing_config_subentries",
        "helpers/test_device_registry.py::test_deleted_device_removing_config_subentries",
        # Area management
        "helpers/test_device_registry.py::test_removing_area_id",
        "helpers/test_device_registry.py::test_removing_area_id_deleted_device",
        # Via device
        "helpers/test_device_registry.py::test_specifying_via_device_create",
        "helpers/test_device_registry.py::test_specifying_via_device_update",
        # Updates
        "helpers/test_device_registry.py::test_update",
        "helpers/test_device_registry.py::test_update_connection",
        "helpers/test_device_registry.py::test_update_remove_config_entries",
        "helpers/test_device_registry.py::test_update_remove_config_subentries",
        "helpers/test_device_registry.py::test_update_suggested_area",
        "helpers/test_device_registry.py::test_update_add_config_entry_disabled_by",
        "helpers/test_device_registry.py::test_update_remove_config_entry_disabled_by",
        # Cleanup
        "helpers/test_device_registry.py::test_cleanup_device_registry",
        "helpers/test_device_registry.py::test_cleanup_device_registry_removes_expired_orphaned_devices",
        "helpers/test_device_registry.py::test_cleanup_startup",
        "helpers/test_device_registry.py::test_cleanup_entity_registry_change",
        # Restore
        "helpers/test_device_registry.py::test_restore_device",
        "helpers/test_device_registry.py::test_restore_disabled_by",
        "helpers/test_device_registry.py::test_restore_shared_device",
        # Creation patterns
        "helpers/test_device_registry.py::test_get_or_create_empty_then_set_default_values",
        "helpers/test_device_registry.py::test_get_or_create_empty_then_update",
        "helpers/test_device_registry.py::test_get_or_create_sets_default_values",
        "helpers/test_device_registry.py::test_verify_suggested_area_does_not_overwrite_area_id",
        # Disable handling
        "helpers/test_device_registry.py::test_disable_config_entry_disables_devices",
        "helpers/test_device_registry.py::test_only_disable_device_if_all_config_entries_are_disabled",
        # Labels
        "helpers/test_device_registry.py::test_removing_labels",
        "helpers/test_device_registry.py::test_removing_labels_deleted_device",
        "helpers/test_device_registry.py::test_entries_for_label",
        # Primary config entry
        "helpers/test_device_registry.py::test_primary_config_entry",
        "helpers/test_device_registry.py::test_update_device_no_connections_or_identifiers",
        # Collision handling
        "helpers/test_device_registry.py::test_device_registry_connections_collision",
        "helpers/test_device_registry.py::test_device_registry_identifiers_collision",
        "helpers/test_device_registry.py::test_device_registry_deleted_device_collision",
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
    # Service Bridge (tests/helpers/test_service.py) - ha-py-bridge crate
    # Tests Python→Rust service registration and calling
    # ==========================================================================
    "service_bridge": [
        "helpers/test_service.py::test_async_get_all_descriptions",
        "helpers/test_service.py::test_register_with_mixed_case",
        "helpers/test_service.py::test_call_context_user_not_exist",
        "helpers/test_service.py::test_not_mutate_input",
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
        # Event commands
        "components/websocket_api/test_commands.py::test_fire_event",
        "components/websocket_api/test_commands.py::test_fire_event_without_data",
        # Service commands
        "components/websocket_api/test_commands.py::test_call_service",
        "components/websocket_api/test_commands.py::test_call_service_blocking",
        "components/websocket_api/test_commands.py::test_call_service_target",
        "components/websocket_api/test_commands.py::test_call_service_target_template",
        "components/websocket_api/test_commands.py::test_call_service_schema_validation_error",
        "components/websocket_api/test_commands.py::test_call_service_error",
        # test_call_service_not_found - tests HA's translation caching, not Rust impl
        # test_call_service_child_not_found - tests HA's translation caching, not Rust impl
        # Subscription commands
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_events",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_events_whitelist",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_events_state_changed",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_entities",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_entities_specific_entities",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_entities_with_filter",
        "components/websocket_api/test_commands.py::test_subscribe_entities_with_unserializable_state",
        "components/websocket_api/test_commands.py::test_subscribe_entities_chained_state_change",
        "components/websocket_api/test_commands.py::test_subscribe_unsubscribe_bootstrap_integrations",
        "components/websocket_api/test_commands.py::test_subscribe_conditions",
        "components/websocket_api/test_commands.py::test_subscribe_triggers",
        "components/websocket_api/test_commands.py::test_subscribe_trigger",
        # State commands
        "components/websocket_api/test_commands.py::test_get_states",
        "components/websocket_api/test_commands.py::test_states_filters_visible",
        "components/websocket_api/test_commands.py::test_get_states_not_allows_nan",
        # Service/config commands
        "components/websocket_api/test_commands.py::test_get_services",
        "components/websocket_api/test_commands.py::test_get_config",
        "components/websocket_api/test_commands.py::test_ping",
        "components/websocket_api/test_commands.py::test_call_service_context_with_user",
        "components/websocket_api/test_commands.py::test_subscribe_requires_admin",
        # Template commands
        "components/websocket_api/test_commands.py::test_render_template_renders_template",
        "components/websocket_api/test_commands.py::test_render_template_with_timeout_and_variables",
        "components/websocket_api/test_commands.py::test_render_template_manual_entity_ids_no_longer_needed",
        "components/websocket_api/test_commands.py::test_render_template_with_error",
        "components/websocket_api/test_commands.py::test_render_template_with_timeout_and_error",
        "components/websocket_api/test_commands.py::test_render_template_strict_with_timeout_and_error",
        "components/websocket_api/test_commands.py::test_render_template_strict_with_timeout_and_error_2",
        "components/websocket_api/test_commands.py::test_render_template_error_in_template_code",
        "components/websocket_api/test_commands.py::test_render_template_error_in_template_code_2",
        "components/websocket_api/test_commands.py::test_render_template_with_delayed_error",
        "components/websocket_api/test_commands.py::test_render_template_with_delayed_error_2",
        "components/websocket_api/test_commands.py::test_render_template_with_timeout",
        "components/websocket_api/test_commands.py::test_render_template_returns_with_match_all",
        # Condition/trigger commands
        "components/websocket_api/test_commands.py::test_test_condition",
        # Script execution
        "components/websocket_api/test_commands.py::test_execute_script",
        # test_execute_script_complex_response - requires hassil/calendar dependency
        "components/websocket_api/test_commands.py::test_execute_script_with_dynamically_validated_action",
        # Config validation
        "components/websocket_api/test_commands.py::test_validate_config_works",
        "components/websocket_api/test_commands.py::test_validate_config_invalid",
        # Message coalescing
        "components/websocket_api/test_commands.py::test_message_coalescing",
        "components/websocket_api/test_commands.py::test_message_coalescing_not_supported_by_websocket_client",
        "components/websocket_api/test_commands.py::test_client_message_coalescing",
        # Integration wait
        "components/websocket_api/test_commands.py::test_wait_integration",
        "components/websocket_api/test_commands.py::test_wait_integration_startup",
        # Target extraction
        "components/websocket_api/test_commands.py::test_extract_from_target",
        "components/websocket_api/test_commands.py::test_extract_from_target_expand_group",
        "components/websocket_api/test_commands.py::test_extract_from_target_missing_entities",
        "components/websocket_api/test_commands.py::test_extract_from_target_empty_target",
        "components/websocket_api/test_commands.py::test_extract_from_target_validation_error",
        # Service lookup
        "components/websocket_api/test_commands.py::test_get_triggers_conditions_for_target",
        "components/websocket_api/test_commands.py::test_get_services_for_target",
        "components/websocket_api/test_commands.py::test_get_services_for_target_caching",
        "components/websocket_api/test_commands.py::test_integration_setup_info",
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

    # ==========================================================================
    # Config Entries REST API (tests/components/config/) - ha-api crate
    # Tests config entries REST endpoints
    # ==========================================================================
    "config_entries_rest": [
        "components/config/test_config_entries.py::test_get_entries",
        "components/config/test_config_entries.py::test_remove_entry",
        "components/config/test_config_entries.py::test_reload_entry",
        "components/config/test_config_entries.py::test_reload_invalid_entry",
        "components/config/test_config_entries.py::test_remove_entry_unauth",
        "components/config/test_config_entries.py::test_reload_entry_unauth",
        "components/config/test_config_entries.py::test_reload_entry_in_failed_state",
        "components/config/test_config_entries.py::test_reload_entry_in_setup_retry",
    ],

    # ==========================================================================
    # Config Flow (tests/components/config/) - ha-api crate
    # Tests config flow initialization, steps, and completion
    # ==========================================================================
    "config_flow": [
        "components/config/test_config_entries.py::test_available_flows",
        "components/config/test_config_entries.py::test_initialize_flow",
        "components/config/test_config_entries.py::test_initialize_flow_unmet_dependency",
        "components/config/test_config_entries.py::test_initialize_flow_unauth",
        "components/config/test_config_entries.py::test_abort",
        "components/config/test_config_entries.py::test_create_account",
        "components/config/test_config_entries.py::test_two_step_flow",
        "components/config/test_config_entries.py::test_continue_flow_unauth",
        "components/config/test_config_entries.py::test_get_progress_index",
        "components/config/test_config_entries.py::test_get_progress_index_unauth",
        "components/config/test_config_entries.py::test_get_progress_flow",
        "components/config/test_config_entries.py::test_get_progress_flow_unauth",
        "components/config/test_config_entries.py::test_get_progress_subscribe",
        "components/config/test_config_entries.py::test_get_progress_subscribe_create_entry",
        "components/config/test_config_entries.py::test_get_progress_subscribe_in_progress",
        "components/config/test_config_entries.py::test_get_progress_subscribe_in_progress_bad_flow",
        "components/config/test_config_entries.py::test_get_progress_subscribe_unauth",
        "components/config/test_config_entries.py::test_ignore_flow",
        "components/config/test_config_entries.py::test_ignore_flow_nonexisting",
        "components/config/test_config_entries.py::test_flow_with_multiple_schema_errors",
        "components/config/test_config_entries.py::test_flow_with_multiple_schema_errors_base",
        "components/config/test_config_entries.py::test_supports_reconfigure",
        "components/config/test_config_entries.py::test_does_not_support_reconfigure",
    ],

    # ==========================================================================
    # Options Flow (tests/components/config/) - ha-api crate
    # Tests options flow for config entries
    # ==========================================================================
    "options_flow": [
        "components/config/test_config_entries.py::test_options_flow",
        "components/config/test_config_entries.py::test_options_flow_unauth",
        "components/config/test_config_entries.py::test_two_step_options_flow",
        "components/config/test_config_entries.py::test_options_flow_with_invalid_data",
    ],

    # ==========================================================================
    # Subentry Flow (tests/components/config/) - ha-api crate
    # Tests subentry config flow
    # ==========================================================================
    "subentry_flow": [
        "components/config/test_config_entries.py::test_subentry_flow",
        "components/config/test_config_entries.py::test_subentry_reconfigure_flow",
        "components/config/test_config_entries.py::test_subentry_flow_abort_duplicate",
        "components/config/test_config_entries.py::test_subentry_does_not_support_reconfigure",
        "components/config/test_config_entries.py::test_subentry_flow_unauth",
        "components/config/test_config_entries.py::test_two_step_subentry_flow",
        "components/config/test_config_entries.py::test_subentry_flow_with_invalid_data",
    ],

    # ==========================================================================
    # Config Entries WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config_entries/subscribe and related WebSocket commands
    # ==========================================================================
    "config_entries_ws": [
        "components/config/test_config_entries.py::test_get_single",
        "components/config/test_config_entries.py::test_update_prefrences",
        "components/config/test_config_entries.py::test_update_entry",
        "components/config/test_config_entries.py::test_update_entry_nonexisting",
        "components/config/test_config_entries.py::test_disable_entry",
        "components/config/test_config_entries.py::test_disable_entry_nonexisting",
        "components/config/test_config_entries.py::test_get_matching_entries_ws",
        "components/config/test_config_entries.py::test_subscribe_entries_ws",
        "components/config/test_config_entries.py::test_subscribe_entries_ws_filtered",
    ],

    # ==========================================================================
    # Config Entries Subentries WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config_entries/subentries/list and related WebSocket commands
    # ==========================================================================
    "config_entries_subentries_ws": [
        "components/config/test_config_entries.py::test_list_subentries",
        "components/config/test_config_entries.py::test_update_subentry",
        "components/config/test_config_entries.py::test_delete_subentry",
    ],

    # ==========================================================================
    # Device Registry WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config/device_registry/list and related WebSocket commands
    # ==========================================================================
    "device_registry_ws": [
        "components/config/test_device_registry.py::test_list_devices",
        "components/config/test_device_registry.py::test_update_device",
        "components/config/test_device_registry.py::test_update_device_labels",
        "components/config/test_device_registry.py::test_remove_config_entry_from_device",
        "components/config/test_device_registry.py::test_remove_config_entry_from_device_fails",
        "components/config/test_device_registry.py::test_remove_config_entry_from_device_if_integration_remove",
    ],

    # ==========================================================================
    # Entity Registry WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config/entity_registry/list and related WebSocket commands
    # Note: These tests depend on Python-side fixtures (mock_registry, etc.)
    # that set up test data. For proxy client testing against Rust server,
    # use tests/integration/test_entity_registry_ws.py instead.
    # ==========================================================================
    "entity_registry_ws": [
        "components/config/test_entity_registry.py::test_list_entities",
        "components/config/test_entity_registry.py::test_list_entities_for_display",
        "components/config/test_entity_registry.py::test_get_entity",
        "components/config/test_entity_registry.py::test_get_entities",
        "components/config/test_entity_registry.py::test_get_nonexisting_entity",
        "components/config/test_entity_registry.py::test_update_entity",
        "components/config/test_entity_registry.py::test_update_entity_require_restart",
        "components/config/test_entity_registry.py::test_update_entity_no_changes",
        "components/config/test_entity_registry.py::test_update_nonexisting_entity",
        "components/config/test_entity_registry.py::test_update_entity_id",
        "components/config/test_entity_registry.py::test_update_existing_entity_id",
        "components/config/test_entity_registry.py::test_update_invalid_entity_id",
        "components/config/test_entity_registry.py::test_remove_entity",
        "components/config/test_entity_registry.py::test_remove_non_existing_entity",
        "components/config/test_entity_registry.py::test_enable_entity_disabled_device",
        "components/config/test_entity_registry.py::test_get_automatic_entity_ids",
    ],

    # ==========================================================================
    # Area Registry WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config/area_registry/list and related WebSocket commands
    # ==========================================================================
    "area_registry_ws": [
        "components/config/test_area_registry.py::test_list_areas",
        "components/config/test_area_registry.py::test_create_area",
        "components/config/test_area_registry.py::test_create_area_with_name_already_in_use",
        "components/config/test_area_registry.py::test_delete_area",
        "components/config/test_area_registry.py::test_delete_non_existing_area",
        "components/config/test_area_registry.py::test_update_area",
        "components/config/test_area_registry.py::test_update_area_with_same_name",
        "components/config/test_area_registry.py::test_update_area_with_name_already_in_use",
        "components/config/test_area_registry.py::test_reorder_areas",
        "components/config/test_area_registry.py::test_reorder_areas_invalid_area_ids",
        "components/config/test_area_registry.py::test_reorder_areas_with_nonexistent_id",
        "components/config/test_area_registry.py::test_reorder_areas_persistence",
    ],

    # ==========================================================================
    # Floor Registry WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config/floor_registry/list and related WebSocket commands
    # ==========================================================================
    "floor_registry_ws": [
        "components/config/test_floor_registry.py::test_list_floors",
        "components/config/test_floor_registry.py::test_create_floor",
        "components/config/test_floor_registry.py::test_create_floor_with_name_already_in_use",
        "components/config/test_floor_registry.py::test_delete_floor",
        "components/config/test_floor_registry.py::test_delete_non_existing_floor",
        "components/config/test_floor_registry.py::test_update_floor",
        "components/config/test_floor_registry.py::test_update_with_name_already_in_use",
        "components/config/test_floor_registry.py::test_reorder_floors",
        "components/config/test_floor_registry.py::test_reorder_floors_invalid_floor_ids",
        "components/config/test_floor_registry.py::test_reorder_floors_with_nonexistent_id",
        "components/config/test_floor_registry.py::test_reorder_floors_persistence",
    ],

    # ==========================================================================
    # Label Registry WebSocket API (tests/components/config/) - ha-api crate
    # Tests the config/label_registry/list and related WebSocket commands
    # ==========================================================================
    "label_registry_ws": [
        "components/config/test_label_registry.py::test_list_labels",
        "components/config/test_label_registry.py::test_create_label",
        "components/config/test_label_registry.py::test_create_label_with_name_already_in_use",
        "components/config/test_label_registry.py::test_delete_label",
        "components/config/test_label_registry.py::test_delete_non_existing_label",
        "components/config/test_label_registry.py::test_update_label",
        "components/config/test_label_registry.py::test_update_with_name_already_in_use",
    ],

    # ==========================================================================
    # Python Shim Layer Tests (python/homeassistant/)
    # These tests run native HA tests with our shim taking precedence
    # ==========================================================================
    "shim_entity": [
        # Entity base class tests - validates our Entity shim
        "helpers/test_entity.py::test_generate_entity_id_requires_hass_or_ids",
        "helpers/test_entity.py::test_generate_entity_id_given_keys",
        "helpers/test_entity.py::test_generate_entity_id_given_hass",
        "helpers/test_entity.py::test_device_class",
        "helpers/test_entity.py::test_capability_attrs",
        "helpers/test_entity.py::test_entity_category_property",
    ],
    "shim_exceptions": [
        # Exception tests - validates our exceptions module
        "test_exceptions.py::test_conditionerror_format",
        "test_exceptions.py::test_template_message",
        "test_exceptions.py::test_home_assistant_error",
    ],

    # ==========================================================================
    # Application Credentials (tests/components/application_credentials/) - ha-api crate
    # Tests OAuth2 client credential management and storage
    # ==========================================================================
    "application_credentials": [
        # Basic WebSocket commands
        "components/application_credentials/test_init.py::test_websocket_list_empty",
        "components/application_credentials/test_init.py::test_websocket_create",
        "components/application_credentials/test_init.py::test_websocket_create_invalid_domain",
        "components/application_credentials/test_init.py::test_websocket_update_not_supported",
        "components/application_credentials/test_init.py::test_websocket_delete",
        "components/application_credentials/test_init.py::test_websocket_delete_item_not_found",
        "components/application_credentials/test_init.py::test_websocket_integration_list",
        "components/application_credentials/test_init.py::test_websocket_create_strips_whitespace",
    ],

    # ==========================================================================
    # Manifest WebSocket API (tests/components/websocket_api/) - ha-api crate
    # Tests manifest/list and manifest/get WebSocket commands
    # ==========================================================================
    "manifest_ws": [
        "components/websocket_api/test_commands.py::test_manifest_list",
        "components/websocket_api/test_commands.py::test_manifest_list_specific_integrations",
        "components/websocket_api/test_commands.py::test_manifest_get",
    ],

    # ==========================================================================
    # Entity Source WebSocket API (tests/components/websocket_api/) - ha-api crate
    # Tests entity/source WebSocket command
    # ==========================================================================
    "entity_source_ws": [
        "components/websocket_api/test_commands.py::test_entity_source_admin",
    ],

    # ==========================================================================
    # Integration Descriptions WebSocket API (tests/components/websocket_api/) - ha-api crate
    # Tests integration/descriptions WebSocket command
    # ==========================================================================
    "integration_descriptions_ws": [
        "components/websocket_api/test_commands.py::test_integration_descriptions",
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

def load_rust_conftest():
    """Load our Rust conftest module by file path to avoid namespace conflicts."""
    conftest_path = get_repo_root() / "tests" / "ha_compat" / "conftest.py"
    spec = importlib.util.spec_from_file_location("rust_conftest", conftest_path)
    rust_conftest = importlib.util.module_from_spec(spec)
    sys.modules["rust_conftest"] = rust_conftest
    spec.loader.exec_module(rust_conftest)
    return rust_conftest


def run_tests(categories: list[str] | None = None, verbose: bool = False) -> int:
    """Run the compatibility tests against Rust implementations.

    Args:
        categories: List of test categories to run, or None for all
        verbose: Enable verbose output

    Returns:
        Exit code (0 for success)
    """
    repo_root = get_repo_root()
    ha_core = get_ha_core_dir()
    shim_path = repo_root / "crates" / "ha-py-bridge" / "python"

    if not ha_core.exists():
        print(f"Error: HA core not found at {ha_core}")
        print("Run: make ha-compat-setup")
        return 1

    # Detect if any shim categories are requested (need shim path in PYTHONPATH)
    shim_categories = [c for c in (categories or []) if c.startswith("shim_")]
    use_shim = bool(shim_categories) or categories is None  # Include shim for --all

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
        for cat, tests in TEST_CATEGORIES.items():
            patterns.extend(tests)

    # Setup PYTHONPATH for imports
    # vendor/ha-core must be in path for HA's test imports to work
    # Our repo root must be in path for our conftest to find ha_core_rs
    # ha_compat directory needs to be on path for pytest to discover our conftest.py
    ha_compat = repo_root / "tests" / "ha_compat"
    pythonpath_parts = [str(ha_compat), str(ha_core), str(repo_root)]
    if use_shim:
        # Put our shim first so it takes precedence over site-packages
        pythonpath_parts.insert(0, str(shim_path))

    # Prepend to sys.path
    for path in reversed(pythonpath_parts):
        if path not in sys.path:
            sys.path.insert(0, path)

    # Set environment variable for Rust components
    os.environ["USE_RUST_COMPONENTS"] = "1"

    # Load our Rust conftest as a plugin
    # This must be done AFTER setting up sys.path
    rust_conftest = load_rust_conftest()

    if rust_conftest._rust_available:
        print(f"Running {len(patterns)} tests against Rust extension...")
        print("=" * 60)
        print("  Rust components ENABLED via ha_core_rs")
        print("  Core types (State, Event, Context) are Rust-backed")
        print("=" * 60)
    else:
        print(f"Running {len(patterns)} tests (Rust NOT available, using Python)...")
    print("")

    # Change to ha-core directory for test discovery
    original_cwd = os.getcwd()
    os.chdir(ha_core)

    try:
        # Import pytest here (after PYTHONPATH is set up)
        import pytest

        # Build pytest args
        pytest_args = [
            "-v" if verbose else "-q",
            "--tb=short",
            "-x",  # Stop on first failure
        ]

        for pattern in patterns:
            pytest_args.append(f"tests/{pattern}")

        # Run pytest with our conftest as a plugin
        exit_code = pytest.main(pytest_args, plugins=[rust_conftest])
        return exit_code
    finally:
        os.chdir(original_cwd)

def main():
    parser = argparse.ArgumentParser(description="Run HA compatibility tests against Rust")
    parser.add_argument("--list", action="store_true", help="List test categories")
    parser.add_argument("--category", "-c", action="append",
                        help="Test category to run (can specify multiple)")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--all", "-a", action="store_true", help="Run all categories")

    args = parser.parse_args()

    if args.list:
        list_categories()
        return 0

    categories = args.category if args.category else None
    if args.all:
        categories = None

    return run_tests(categories=categories, verbose=args.verbose)

if __name__ == "__main__":
    sys.exit(main())
