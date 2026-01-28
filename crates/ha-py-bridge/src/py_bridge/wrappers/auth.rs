//! AuthWrapper - authentication system wrapper for Python integrations
//!
//! Provides Python-compatible auth methods that integrations like Cast need.

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Python wrapper for the Home Assistant auth manager
///
/// Provides async methods for user management that integrations need.
/// For now, returns minimal mock data to allow integrations to load.
#[pyclass(name = "AuthManager")]
pub struct AuthWrapper {
    /// Cached users (user_id -> User object)
    users: Py<PyDict>,
    /// Auth providers list
    auth_providers: PyObject,
}

impl AuthWrapper {
    pub fn new(py: Python<'_>) -> PyResult<Self> {
        let users = PyDict::new_bound(py);

        // Create empty auth_providers list
        let auth_providers = py.eval_bound("[]", None, None)?.into_py(py);

        Ok(Self {
            users: users.unbind(),
            auth_providers,
        })
    }
}

#[pymethods]
impl AuthWrapper {
    /// Get a user by ID
    ///
    /// Returns None if user not found, or a User object if found.
    #[pyo3(name = "async_get_user")]
    fn async_get_user<'py>(&self, py: Python<'py>, user_id: String) -> PyResult<Bound<'py, PyAny>> {
        let users = self.users.bind(py);

        // Check if we have this user cached
        if let Some(user) = users.get_item(&user_id)? {
            // Return a coroutine that immediately returns the user
            let code = r#"
async def get_user(user):
    return user
"#;
            let globals = PyDict::new_bound(py);
            py.run_bound(code, Some(&globals), None)?;
            let get_fn = globals.get_item("get_user")?.unwrap();
            return get_fn.call1((user,));
        }

        // Return a coroutine that returns None
        let code = r#"
async def get_none():
    return None
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;
        let get_fn = globals.get_item("get_none")?.unwrap();
        get_fn.call0()
    }

    /// Get all users
    #[pyo3(name = "async_get_users")]
    fn async_get_users<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let users = self.users.bind(py);

        // Collect all users into a list
        let user_list: Vec<_> = users.values().iter().collect();

        let code = r#"
async def get_users(users):
    return list(users)
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;
        let get_fn = globals.get_item("get_users")?.unwrap();
        get_fn.call1((user_list,))
    }

    /// Create a system user
    #[pyo3(name = "async_create_system_user", signature = (name, group_ids=None, local_only=None))]
    fn async_create_system_user<'py>(
        &self,
        py: Python<'py>,
        name: String,
        group_ids: Option<Vec<String>>,
        local_only: Option<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let users = self.users.bind(py);

        // Generate a unique user ID
        let user_id = format!("system_{}", ulid::Ulid::new().to_string());

        // Create a simple User-like object
        let code = r#"
class User:
    def __init__(self, id, name, is_owner, is_active, is_admin, system_generated, credentials, group_ids, local_only):
        self.id = id
        self.name = name
        self.is_owner = is_owner
        self.is_active = is_active
        self.is_admin = is_admin
        self.system_generated = system_generated
        self.credentials = credentials
        self.group_ids = group_ids
        self.local_only = local_only

        # Create a basic permissions object
        class Permissions:
            def access_all_entities(self, policy):
                return True
            def check_entity(self, entity_id, policy):
                return True
        self.permissions = Permissions()

async def create_user(id, name, group_ids, local_only):
    return User(
        id=id,
        name=name,
        is_owner=False,
        is_active=True,
        is_admin=True,
        system_generated=True,
        credentials=[],
        group_ids=group_ids or [],
        local_only=local_only or False,
    )
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;
        let create_fn = globals.get_item("create_user")?.unwrap();
        let coro = create_fn.call1((&user_id, name.clone(), group_ids, local_only))?;

        // Store user for future lookups - we'll need to await the coroutine first
        // For now, create the user object directly and store it
        let user_class = globals.get_item("User")?.unwrap();
        let user = user_class.call1((
            &user_id,
            name,
            false,                            // is_owner
            true,                             // is_active
            true,                             // is_admin
            true,                             // system_generated
            py.eval_bound("[]", None, None)?, // credentials
            py.eval_bound("[]", None, None)?, // group_ids
            false,                            // local_only
        ))?;
        users.set_item(&user_id, &user)?;

        Ok(coro)
    }

    /// Create a refresh token for a user
    #[pyo3(name = "async_create_refresh_token", signature = (user, client_id=None, client_name=None, client_icon=None, token_type=None, access_token_expiration=None, credential=None))]
    fn async_create_refresh_token<'py>(
        &self,
        py: Python<'py>,
        user: PyObject,
        client_id: Option<String>,
        client_name: Option<String>,
        client_icon: Option<String>,
        token_type: Option<String>,
        access_token_expiration: Option<f64>,
        credential: Option<PyObject>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let token_id = ulid::Ulid::new().to_string();
        let token = format!("rt_{}", ulid::Ulid::new().to_string());

        // Create a RefreshToken-like object
        let code = r#"
class RefreshToken:
    def __init__(self, id, token, user, client_id, client_name, client_icon, token_type, access_token_expiration):
        self.id = id
        self.token = token
        self.user = user
        self.client_id = client_id
        self.client_name = client_name
        self.client_icon = client_icon
        self.token_type = token_type
        self.access_token_expiration = access_token_expiration

async def create_token(id, token, user, client_id, client_name, client_icon, token_type, expiration):
    return RefreshToken(id, token, user, client_id, client_name, client_icon, token_type, expiration)
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;
        let create_fn = globals.get_item("create_token")?.unwrap();
        create_fn.call1((
            token_id,
            token,
            user,
            client_id,
            client_name,
            client_icon,
            token_type.unwrap_or_else(|| "normal".to_string()),
            access_token_expiration.unwrap_or(1800.0),
        ))
    }

    /// Remove a user
    #[pyo3(name = "async_remove_user")]
    fn async_remove_user<'py>(
        &self,
        py: Python<'py>,
        user: PyObject,
    ) -> PyResult<Bound<'py, PyAny>> {
        let users = self.users.bind(py);

        // Get user ID and remove from cache
        if let Ok(user_id) = user.getattr(py, "id") {
            if let Ok(id_str) = user_id.extract::<String>(py) {
                let _ = users.del_item(&id_str);
            }
        }

        let code = r#"
async def remove():
    pass
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;
        let remove_fn = globals.get_item("remove")?.unwrap();
        remove_fn.call0()
    }

    /// Validate an access token
    ///
    /// This is a synchronous method that returns a RefreshToken or None.
    #[pyo3(name = "async_validate_access_token")]
    fn async_validate_access_token(
        &self,
        _py: Python<'_>,
        _access_token: String,
    ) -> Option<PyObject> {
        // For now, return None - the Rust WebSocket handler handles its own auth
        None
    }

    /// Register a callback for token revocation
    #[pyo3(name = "async_register_revoke_token_callback")]
    fn async_register_revoke_token_callback<'py>(
        &self,
        py: Python<'py>,
        _token_id: String,
        _callback: PyObject,
    ) -> PyResult<PyObject> {
        // Return a no-op unsubscribe function
        let code = r#"
def noop():
    pass
"#;
        let globals = PyDict::new_bound(py);
        py.run_bound(code, Some(&globals), None)?;
        let noop = globals.get_item("noop")?.unwrap();
        Ok(noop.into_py(py))
    }

    /// Get auth providers list
    #[getter]
    fn auth_providers(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.auth_providers.clone_ref(py))
    }
}
