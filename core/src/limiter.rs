use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use log::{debug, error, info};

use crate::CoreError;

#[derive(Clone, Debug)]
pub struct LimitController {
    pub max_grants_per_user: u64,
    inner: Arc<RwLock<LimiterControllerInner>>,
}

#[derive(Clone, Debug)]
pub struct LimiterControllerInner {
    pub state: HashMap<String, u64>,
}

pub struct LimitGrant {
    key: String,
    inner: Arc<RwLock<LimiterControllerInner>>,
}

impl LimitController {
    pub fn new(max_grants_per_user: u64) -> Self {
        debug!("Initializing LimitController with {max_grants_per_user} grants.");
        Self {
            max_grants_per_user,
            inner: Arc::new(RwLock::new(LimiterControllerInner {
                state: HashMap::new(),
            })),
        }
    }

    /// Try to create a new grant for key `key`.
    /// If this user already has more than `max_grants_per_user`, None will be returned.
    ///
    /// Whenever one of these grants is dropped, the grant count for that key will be
    /// decreased.
    pub fn new_grant_for_key(&self, key: &str) -> Result<LimitGrant, CoreError> {
        // Avoid performing a write lock if this user already exceeded the limit.
        {
            let inner = self.inner.read().map_err(|e| {
                error!("cannot unlock state: {}", e);
                CoreError::PoisonError
            })?;
            if let Some(grants) = inner.state.get(key) {
                if *grants >= self.max_grants_per_user {
                    info!(
                        "key {} currently holds {} grants, max is {}",
                        key, grants, self.max_grants_per_user
                    );
                    return Err(CoreError::GrantError(key.to_string(), *grants));
                }
            }
        }

        let mut inner = self.inner.write().map_err(|e| {
            error!("cannot unlock state: {}", e);
            CoreError::PoisonError
        })?;
        let state = &mut inner.state;
        let current_grant_count = state.entry(key.to_string()).or_default();
        *current_grant_count += 1;
        Ok(LimitGrant {
            key: key.to_string(),
            inner: self.inner.clone(),
        })
    }
}

impl Drop for LimitGrant {
    fn drop(&mut self) {
        let mut inner = match self.inner.write() {
            Ok(inner) => inner,
            Err(e) => {
                error!("state was poisoned: {e:?}, recovering");
                e.into_inner()
            }
        };
        let state = &mut inner.state;
        if state.get(&self.key) == Some(&1) {
            state.remove(&self.key);
        } else if let Some(grants) = state.get_mut(&self.key) {
            *grants -= 1;
        }
    }
}
