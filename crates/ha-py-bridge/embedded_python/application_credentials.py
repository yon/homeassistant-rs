"""Application credentials injection for OAuth config flows.

Called from Rust via py.run_bound() with globals: _hass, _credentials_json.
Self-executes _inject_credentials(_hass, _credentials_json) at the end.
"""
import logging

_LOGGER = logging.getLogger(__name__)

def _inject_credentials(hass, credentials_json):
    """Inject application credentials into hass.data for OAuth flows.

    This sets up the application_credentials component's data structure
    so Python OAuth config flows can find credentials via the standard
    async_get_implementations() call path.
    """
    import json

    credentials = json.loads(credentials_json)
    _LOGGER.info(f"Injecting {len(credentials)} application credentials into hass.data")

    if not credentials:
        _LOGGER.debug("No credentials to inject")
        return True

    # Set up the loader's data structures (required for async_get_integration to work)
    try:
        from homeassistant import loader
        if loader.DATA_COMPONENTS not in hass.data:
            hass.data[loader.DATA_COMPONENTS] = {}
        if loader.DATA_INTEGRATIONS not in hass.data:
            hass.data[loader.DATA_INTEGRATIONS] = {}
        if loader.DATA_MISSING_PLATFORMS not in hass.data:
            hass.data[loader.DATA_MISSING_PLATFORMS] = {}
        if loader.DATA_PRELOAD_PLATFORMS not in hass.data:
            hass.data[loader.DATA_PRELOAD_PLATFORMS] = loader.BASE_PRELOAD_PLATFORMS.copy()
        _LOGGER.debug("Set up loader data structures in hass.data")
    except Exception as e:
        _LOGGER.warning(f"Could not set up loader data structures: {e}")

    # Import the components we need to mock
    try:
        from homeassistant.helpers import config_entry_oauth2_flow
    except ImportError as e:
        _LOGGER.debug(f"Could not import config_entry_oauth2_flow: {e}")
        return False

    # Create the DATA_PROVIDERS dict if it doesn't exist
    # This is where async_get_implementations looks for implementation providers
    providers_key = config_entry_oauth2_flow.DATA_PROVIDERS
    if providers_key not in hass.data:
        hass.data[providers_key] = {}

    # Create the DATA_IMPLEMENTATIONS dict if it doesn't exist
    # This is where implementations are cached per domain
    impl_key = config_entry_oauth2_flow.DATA_IMPLEMENTATIONS
    if impl_key not in hass.data:
        hass.data[impl_key] = {}

    # Group credentials by domain
    creds_by_domain = {}
    for cred in credentials:
        domain = cred['domain']
        if domain not in creds_by_domain:
            creds_by_domain[domain] = []
        creds_by_domain[domain].append(cred)

    # For each domain with credentials, register an implementation provider
    for domain, domain_creds in creds_by_domain.items():
        _LOGGER.info(f"Registering {len(domain_creds)} credentials for domain: {domain}")

        # Create a closure that captures domain_creds
        # This is a regular function that returns an async provider function
        # The async provider will be called later when the OAuth flow needs implementations
        def make_provider(captured_creds):
            async def provider(hass, domain):
                """Provide OAuth implementations for this domain."""
                implementations = []

                try:
                    # Try to load the integration's application_credentials platform
                    # to get the authorization server URLs
                    from homeassistant.loader import async_get_integration
                    integration = await async_get_integration(hass, domain)
                    platform = await integration.async_get_platform("application_credentials")

                    if hasattr(platform, "async_get_authorization_server"):
                        auth_server = await platform.async_get_authorization_server(hass)

                        for cred in captured_creds:
                            impl = config_entry_oauth2_flow.LocalOAuth2Implementation(
                                hass,
                                cred.get('auth_domain') or cred['id'],
                                cred['client_id'],
                                cred['client_secret'],
                                auth_server.authorize_url,
                                auth_server.token_url,
                            )
                            implementations.append(impl)
                            _LOGGER.info(f"Created OAuth implementation for {domain} with client_id: {cred['client_id'][:20]}...")
                    elif hasattr(platform, "async_get_auth_implementation"):
                        # Some integrations provide custom implementations
                        for cred in captured_creds:
                            # Create a ClientCredential-like object
                            class ClientCredential:
                                def __init__(self, client_id, client_secret, name=None):
                                    self.client_id = client_id
                                    self.client_secret = client_secret
                                    self.name = name

                            credential = ClientCredential(
                                cred['client_id'],
                                cred['client_secret'],
                                cred.get('name')
                            )
                            impl = await platform.async_get_auth_implementation(
                                hass,
                                cred.get('auth_domain') or cred['id'],
                                credential
                            )
                            implementations.append(impl)
                            _LOGGER.info(f"Created custom OAuth implementation for {domain}")
                except Exception as e:
                    _LOGGER.warning(f"Could not create OAuth implementation for {domain}: {e}")
                    import traceback
                    traceback.print_exc()

                return implementations
            return provider

        # Get the async provider function (not a coroutine, just a function that returns one)
        provider = make_provider(domain_creds)

        # Register via the official API
        config_entry_oauth2_flow.async_add_implementation_provider(hass, domain, provider)

    return True

_inject_credentials(_hass, _credentials_json)
