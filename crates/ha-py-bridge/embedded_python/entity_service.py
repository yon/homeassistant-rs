"""Synchronous entity service dispatch.

Called from Rust via py.run_bound() with CONFIG_ENTRIES_GLOBALS context.
Expects: _entity_registry, _hass, _LOGGER, _update_entity_state_sync (or defines them).
"""


def _call_entity_service_sync(entity_id, service, kwargs):
    """Synchronous wrapper for calling entity services.

    Instead of calling the entity's async methods (which require full HA infrastructure),
    we directly modify the entity attributes based on the service and update state.
    """
    entity = _entity_registry.get(entity_id)
    if entity is None:
        # Entity not in Python registry — try direct state store fallback
        return _call_service_via_state_store(entity_id, service, kwargs)

    domain = entity_id.split('.')[0]

    try:
        # Handle common services by directly modifying entity attributes
        if service in ('turn_on', 'turn_off', 'toggle'):
            if hasattr(entity, '_attr_is_on'):
                if service == 'turn_on':
                    entity._attr_is_on = True
                elif service == 'turn_off':
                    entity._attr_is_on = False
                elif service == 'toggle':
                    entity._attr_is_on = not entity._attr_is_on
                _LOGGER.debug(f"Set {entity_id}._attr_is_on = {entity._attr_is_on}")
            elif hasattr(entity, '_is_on'):
                if service == 'turn_on':
                    entity._is_on = True
                elif service == 'turn_off':
                    entity._is_on = False
                elif service == 'toggle':
                    entity._is_on = not entity._is_on
                _LOGGER.debug(f"Set {entity_id}._is_on = {entity._is_on}")
            else:
                _LOGGER.warning(f"Entity {entity_id} has no _attr_is_on or _is_on attribute")
                return False

            # Handle brightness for turn_on
            if service == 'turn_on' and 'brightness' in kwargs:
                if hasattr(entity, '_attr_brightness'):
                    entity._attr_brightness = kwargs['brightness']

        elif service == 'lock':
            if hasattr(entity, '_attr_is_locked'):
                entity._attr_is_locked = True
        elif service == 'unlock':
            if hasattr(entity, '_attr_is_locked'):
                entity._attr_is_locked = False
        elif service == 'set_value' and domain == 'number':
            if hasattr(entity, '_attr_native_value'):
                entity._attr_native_value = kwargs.get('value')
        elif service == 'select_option' and domain == 'select':
            if hasattr(entity, '_attr_current_option'):
                entity._attr_current_option = kwargs.get('option')
        elif service == 'press' and domain == 'button':
            # Button press doesn't change state, just acknowledge
            pass
        else:
            _LOGGER.warning(f"Service {service} not implemented for direct attribute modification")
            return False

        # Update state in Rust state machine
        _update_entity_state_sync(entity)
        return True
    except Exception as e:
        _LOGGER.error(f"Error calling {service} on {entity_id}: {e}")
        import traceback
        traceback.print_exc()
        return False

def _update_entity_state_sync(entity):
    """Synchronously update the state of an entity in Rust state machine."""
    if _hass is None or not hasattr(entity, 'entity_id'):
        return

    entity_id = entity.entity_id
    domain = entity_id.split('.')[0]

    # Determine state based on domain and entity attributes
    state = None
    if domain in ('light', 'switch', 'fan', 'siren', 'humidifier'):
        if hasattr(entity, '_attr_is_on'):
            state = 'on' if entity._attr_is_on else 'off'
        elif hasattr(entity, '_is_on'):
            state = 'on' if entity._is_on else 'off'
        else:
            state = 'off'
    elif domain == 'lock':
        if hasattr(entity, '_attr_is_locked'):
            state = 'locked' if entity._attr_is_locked else 'unlocked'
        else:
            state = 'unknown'
    elif domain in ('sensor', 'number'):
        if hasattr(entity, '_attr_native_value'):
            state = str(entity._attr_native_value) if entity._attr_native_value is not None else 'unknown'
        else:
            state = 'unknown'
    elif domain == 'select':
        if hasattr(entity, '_attr_current_option'):
            state = str(entity._attr_current_option) if entity._attr_current_option else 'unknown'
        else:
            state = 'unknown'
    elif domain == 'binary_sensor':
        if hasattr(entity, '_attr_is_on'):
            state = 'on' if entity._attr_is_on else 'off'
        else:
            state = 'off'
    else:
        state = 'unknown'

    # Build attributes dict
    attributes = {}
    if hasattr(entity, '_attr_brightness') and entity._attr_brightness is not None:
        attributes['brightness'] = entity._attr_brightness
    if hasattr(entity, '_attr_color_mode') and entity._attr_color_mode is not None:
        cm = entity._attr_color_mode
        attributes['color_mode'] = cm.value if hasattr(cm, 'value') else str(cm)
    if hasattr(entity, '_attr_hs_color') and entity._attr_hs_color is not None:
        attributes['hs_color'] = list(entity._attr_hs_color)

    # Get friendly_name, checking for UndefinedType sentinel values
    def _is_valid_name(value):
        if value is None:
            return False
        type_str = str(type(value))
        if 'UndefinedType' in type_str or str(value) == 'UndefinedType._singleton':
            return False
        return True

    if hasattr(entity, '_attr_friendly_name') and _is_valid_name(entity._attr_friendly_name):
        attributes['friendly_name'] = str(entity._attr_friendly_name)
    elif hasattr(entity, 'name'):
        try:
            name = entity.name
            if _is_valid_name(name):
                attributes['friendly_name'] = str(name)
        except:
            pass

    # Update state in Rust state machine
    if hasattr(_hass, 'states') and hasattr(_hass.states, 'set'):
        _hass.states.set(entity_id, state, attributes)
        _LOGGER.info(f"Updated state: {entity_id} = {state}")


def _call_service_via_state_store(entity_id, service, kwargs):
    """Fall back to direct state store manipulation for entities without Python objects.

    Many entities exist only in the Rust state store (loaded from the entity registry
    on disk) without corresponding Python entity objects. For these, we can still
    handle basic services by reading/writing state directly.
    """
    if _hass is None or not hasattr(_hass, 'states') or not hasattr(_hass.states, 'set'):
        _LOGGER.warning(f"Cannot call service on {entity_id}: state store not available")
        return False

    # Check if the entity exists in the state store
    current = _hass.states.get(entity_id) if hasattr(_hass.states, 'get') else None
    if current is None:
        _LOGGER.warning(f"Entity not found in registry or state store: {entity_id}")
        return False

    current_state = current.get('state', 'unknown') if isinstance(current, dict) else getattr(current, 'state', 'unknown')
    current_attrs = current.get('attributes', {}) if isinstance(current, dict) else {}

    domain = entity_id.split('.')[0]
    new_state = None

    if service == 'turn_on':
        new_state = 'on'
    elif service == 'turn_off':
        new_state = 'off'
    elif service == 'toggle':
        if domain in ('light', 'switch', 'fan', 'siren', 'humidifier'):
            new_state = 'off' if current_state == 'on' else 'on'
        elif domain == 'lock':
            new_state = 'unlocked' if current_state == 'locked' else 'locked'
        else:
            new_state = 'off' if current_state == 'on' else 'on'
    elif service == 'lock':
        new_state = 'locked'
    elif service == 'unlock':
        new_state = 'unlocked'
    elif service == 'open':
        if domain == 'cover':
            new_state = 'open'
        elif domain == 'lock':
            new_state = 'unlocked'
        elif domain == 'valve':
            new_state = 'open'
    elif service == 'close_cover':
        new_state = 'closed'
    elif service == 'open_valve':
        new_state = 'open'
    elif service == 'close_valve':
        new_state = 'closed'
    else:
        _LOGGER.debug(f"Service {service} not handled via state store fallback for {entity_id}")
        return False

    if new_state is not None:
        # Merge brightness into attributes for turn_on
        attrs = dict(current_attrs)
        if service == 'turn_on' and 'brightness' in kwargs:
            attrs['brightness'] = kwargs['brightness']
        _hass.states.set(entity_id, new_state, attrs)
        _LOGGER.info(f"Updated state via fallback: {entity_id} = {new_state}")
        return True

    return False
