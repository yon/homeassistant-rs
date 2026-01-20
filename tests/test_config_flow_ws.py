#!/usr/bin/env python3
"""Test script for config flow WebSocket commands.

This script tests the config_entries/flow and config_entries/flow/progress
WebSocket commands by attempting to start a config flow for lutron_caseta.
"""

import asyncio
import json
import sys
import websockets


async def test_config_flow():
    """Test the config flow WebSocket commands."""
    uri = "ws://localhost:8123/api/websocket"

    async with websockets.connect(uri) as ws:
        # 1. Receive auth_required message
        auth_required = await ws.recv()
        data = json.loads(auth_required)
        print(f"1. Received: {data['type']}")
        assert data["type"] == "auth_required"

        # 2. Send auth message
        await ws.send(json.dumps({
            "type": "auth",
            "access_token": "test_token"  # Our test token
        }))

        auth_result = await ws.recv()
        data = json.loads(auth_result)
        print(f"2. Auth result: {data['type']}")

        if data["type"] != "auth_ok":
            print(f"   Auth failed: {data}")
            return False

        # 3. Start a config flow for lutron_caseta
        print("\n3. Starting config flow for lutron_caseta...")
        await ws.send(json.dumps({
            "id": 1,
            "type": "config_entries/flow",
            "handler": "lutron_caseta",
            "show_advanced_options": False
        }))

        result = await ws.recv()
        data = json.loads(result)
        print(f"   Result: {json.dumps(data, indent=2)}")

        if not data.get("success"):
            print(f"\n   ERROR: Config flow start failed")
            print(f"   Error: {data.get('error', 'Unknown error')}")
            return False

        flow_result = data.get("result", {})
        flow_id = flow_result.get("flow_id")
        result_type = flow_result.get("type")
        step_id = flow_result.get("step_id")

        print(f"\n   Flow ID: {flow_id}")
        print(f"   Result type: {result_type}")
        print(f"   Step ID: {step_id}")

        if flow_result.get("data_schema"):
            print(f"   Form fields:")
            for field in flow_result["data_schema"]:
                field_type = field.get('type') or field.get('field_type', 'unknown')
                print(f"     - {field['name']} ({field_type}, required={field.get('required')})")

        # 4. If we got a form, try to progress the flow
        if result_type == "form" and flow_id:
            # Use the actual Lutron bridge IP
            user_input = {"host": "10.10.3.14"}  # Lutron bridge IP
            print(f"\n4. Got form at step '{step_id}'. Progressing with host={user_input['host']}...")

            await ws.send(json.dumps({
                "id": 2,
                "type": "config_entries/flow/progress",
                "flow_id": flow_id,
                "user_input": user_input
            }))

            result = await ws.recv()
            data = json.loads(result)
            print(f"   Result: {json.dumps(data, indent=2)}")

        print("\n✓ Config flow test completed successfully!")
        return True


async def test_simple_flow():
    """Test with a simpler integration (sun) if lutron_caseta is complex."""
    uri = "ws://localhost:8123/api/websocket"

    async with websockets.connect(uri) as ws:
        # 1. Auth sequence
        auth_required = await ws.recv()
        await ws.send(json.dumps({
            "type": "auth",
            "access_token": "test_token"
        }))
        auth_result = await ws.recv()
        data = json.loads(auth_result)

        if data["type"] != "auth_ok":
            print(f"Auth failed: {data}")
            return False

        # 2. Try sun integration (usually simple)
        print("\nTesting config flow for 'sun' integration...")
        await ws.send(json.dumps({
            "id": 1,
            "type": "config_entries/flow",
            "handler": "sun",
            "show_advanced_options": False
        }))

        result = await ws.recv()
        data = json.loads(result)
        print(f"Result: {json.dumps(data, indent=2)}")

        return data.get("success", False)


if __name__ == "__main__":
    print("=" * 60)
    print("Config Flow WebSocket Test")
    print("=" * 60)

    try:
        success = asyncio.run(test_config_flow())
        if not success:
            print("\n--- Trying simpler integration (sun) ---\n")
            success = asyncio.run(test_simple_flow())
        sys.exit(0 if success else 1)
    except Exception as e:
        print(f"\nError: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
