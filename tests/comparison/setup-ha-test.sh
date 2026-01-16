#!/bin/bash
#
# Setup Home Assistant Test Instance
#
# This script creates the necessary configuration files for a vanilla
# Home Assistant instance that can be used for API comparison testing.
#
# The instance is pre-configured with:
# - A test user (admin/test-password-123)
# - A long-lived access token for API access
# - Onboarding marked as complete
# - Demo entities loaded
#
# Usage:
#   ./setup-ha-test.sh [config-dir]
#
# Default config-dir is ./ha-config

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_DIR="${1:-$SCRIPT_DIR/ha-config}"
STORAGE_DIR="$CONFIG_DIR/.storage"

# Test credentials (DO NOT use in production!)
TEST_USER_ID="test-user-id-12345678"
TEST_USER_NAME="admin"
# Password: test-password-123
# This is a bcrypt hash - HA uses bcrypt for password storage
TEST_PASSWORD_HASH='$2b$12$LQvMTdT5zJ5vq5u1YdWPKOqGjqTtKNgWPBkGqvKn5MWONcZuGvKL2'
TEST_REFRESH_TOKEN_ID="test-refresh-token-id-123"
TEST_ACCESS_TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.dGVzdC10b2tlbi1mb3ItY2ktdGVzdGluZw.test"
# This is the token that will be used for API calls
# Format: base64(token_id)
LONG_LIVED_TOKEN_ID="test-long-lived-token-id-456"
LONG_LIVED_ACCESS_TOKEN="test_api_token_for_comparison_testing_do_not_use_in_production"

echo "Setting up Home Assistant test instance..."
echo "Config directory: $CONFIG_DIR"

# Create directories
mkdir -p "$STORAGE_DIR"

# Create configuration.yaml
cat > "$CONFIG_DIR/configuration.yaml" << 'EOF'
# Home Assistant Test Configuration
# This is a minimal configuration for API comparison testing.
# DO NOT use in production!

homeassistant:
  name: Test Home
  unit_system: metric
  time_zone: UTC
  latitude: 51.5074
  longitude: -0.1278
  elevation: 11
  currency: USD
  country: US
  # Auth providers - trusted_networks allows Docker/localhost without auth
  auth_providers:
    - type: trusted_networks
      trusted_networks:
        - 127.0.0.1/32
        - ::1/128
        - 192.168.0.0/16
        - 172.16.0.0/12
      allow_bypass_login: true
    - type: homeassistant

# Enable API
api:

# Enable frontend for debugging
http:
  server_port: 8123
  # Allow requests from Docker network and localhost without auth for testing
  trusted_proxies:
    - 127.0.0.1
    - 192.168.0.0/16
    - 172.16.0.0/12
  use_x_forwarded_for: true

# Logging
logger:
  default: info
  logs:
    homeassistant.components.api: debug

# Demo entities for testing
demo:

# History and recorder for testing
recorder:
  purge_keep_days: 1

# Automation (empty but enabled)
automation: []

# Script (empty but enabled)
script: []

# Scene (empty but enabled)
scene: []
EOF

# Create auth storage (users and credentials)
cat > "$STORAGE_DIR/auth" << EOF
{
  "version": 1,
  "minor_version": 2,
  "key": "auth",
  "data": {
    "users": [
      {
        "id": "$TEST_USER_ID",
        "group_ids": ["system-admin"],
        "is_owner": true,
        "is_active": true,
        "name": "Test Admin",
        "system_generated": false,
        "local_only": false
      }
    ],
    "groups": [
      {
        "id": "system-admin",
        "name": "Administrators"
      },
      {
        "id": "system-users",
        "name": "Users"
      },
      {
        "id": "system-read-only",
        "name": "Read Only"
      }
    ],
    "credentials": [
      {
        "id": "credential-id-123",
        "user_id": "$TEST_USER_ID",
        "auth_provider_type": "homeassistant",
        "auth_provider_id": null,
        "data": {
          "username": "$TEST_USER_NAME"
        }
      }
    ],
    "refresh_tokens": [
      {
        "id": "$TEST_REFRESH_TOKEN_ID",
        "user_id": "$TEST_USER_ID",
        "client_id": null,
        "client_name": null,
        "client_icon": null,
        "token_type": "normal",
        "created_at": "2026-01-01T00:00:00.000000+00:00",
        "access_token_expiration": 1800,
        "token": "$TEST_ACCESS_TOKEN",
        "jwt_key": "test-jwt-key-for-ci-testing",
        "last_used_at": null,
        "last_used_ip": null,
        "credential_id": "credential-id-123",
        "version": null
      },
      {
        "id": "$LONG_LIVED_TOKEN_ID",
        "user_id": "$TEST_USER_ID",
        "client_id": "https://homeassistant-rs.test",
        "client_name": "CI Test Token",
        "client_icon": null,
        "token_type": "long_lived_access_token",
        "created_at": "2026-01-01T00:00:00.000000+00:00",
        "access_token_expiration": 315360000,
        "token": "$LONG_LIVED_ACCESS_TOKEN",
        "jwt_key": "test-jwt-key-long-lived",
        "last_used_at": null,
        "last_used_ip": null,
        "credential_id": null,
        "version": null
      }
    ]
  }
}
EOF

# Create auth provider storage (username/password)
cat > "$STORAGE_DIR/auth_provider.homeassistant" << EOF
{
  "version": 1,
  "minor_version": 1,
  "key": "auth_provider.homeassistant",
  "data": {
    "users": [
      {
        "username": "$TEST_USER_NAME",
        "password": "$TEST_PASSWORD_HASH"
      }
    ]
  }
}
EOF

# Mark onboarding as complete
cat > "$STORAGE_DIR/onboarding" << EOF
{
  "version": 4,
  "minor_version": 1,
  "key": "onboarding",
  "data": {
    "done": [
      "user",
      "core_config",
      "analytics",
      "integration"
    ]
  }
}
EOF

# Create core config
cat > "$STORAGE_DIR/core.config" << EOF
{
  "version": 1,
  "minor_version": 3,
  "key": "core.config",
  "data": {
    "latitude": 51.5074,
    "longitude": -0.1278,
    "elevation": 11,
    "unit_system_v2": "metric",
    "location_name": "Test Home",
    "time_zone": "UTC",
    "external_url": null,
    "internal_url": null,
    "currency": "USD",
    "country": "US",
    "language": "en"
  }
}
EOF

# Create person storage (required for HA to start properly)
cat > "$STORAGE_DIR/person" << EOF
{
  "version": 2,
  "minor_version": 1,
  "key": "person",
  "data": {
    "items": [
      {
        "id": "person-test-admin",
        "user_id": "$TEST_USER_ID",
        "name": "Test Admin",
        "picture": null,
        "device_trackers": []
      }
    ],
    "storage_version": 2
  }
}
EOF

# Create a placeholder token file - real token generated after Docker starts
# The token will be generated inside the container which has PyJWT
cat > "$CONFIG_DIR/test-token.txt" << EOF
PLACEHOLDER_GENERATE_AFTER_START
EOF

# Create a script to generate the token inside Docker
cat > "$CONFIG_DIR/generate-token.py" << 'PYEOF'
#!/usr/bin/env python3
"""Generate a valid JWT token for API access."""
import jwt
from datetime import datetime, timedelta, timezone

now = datetime.now(timezone.utc)
payload = {
    'iss': 'test-long-lived-token-id-456',
    'iat': now,
    'exp': now + timedelta(days=365),
}
print(jwt.encode(payload, 'test-jwt-key-long-lived', algorithm='HS256'))
PYEOF
chmod +x "$CONFIG_DIR/generate-token.py"

echo ""
echo "Home Assistant test instance configured!"
echo ""
echo "Credentials:"
echo "  Username: $TEST_USER_NAME"
echo "  Password: test-password-123"
echo ""
echo "Token file: $CONFIG_DIR/test-token.txt (generated after Docker starts)"
echo ""
echo "To start: docker compose up -d"
echo "To access: http://localhost:18123"
