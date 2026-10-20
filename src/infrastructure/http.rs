//! Minimal HTTP seam so release and download logic never touch the network in tests.

use std::time::Duration;

use crate::domain::error::UpdateError;

pub trait HttpGet {
    /// Fetches `url`, failing when the body exceeds `max_bytes`.
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, UpdateError>;
}

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .user_agent(concat!("cursorswap/", env!("CARGO_PKG_VERSION")))
            .https_only(true)
            .build();
        UreqHttp {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl HttpGet for UreqHttp {
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, UpdateError> {
        let network = |e: ureq::Error| UpdateError::Network(e.to_string());
        let mut response = self.agent.get(url).call().map_err(network)?;
        response
            .body_mut()
            .with_config()
            .limit(max_bytes)
            .read_to_vec()
            .map_err(network)
    }
}

#[cfg(test)]
pub mod fake {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    pub struct FakeHttp {
        pub responses: HashMap<String, Result<Vec<u8>, UpdateError>>,
        pub requests: RefCell<Vec<(String, u64)>>,
    }

    impl FakeHttp {
        pub fn with(mut self, url: &str, body: &[u8]) -> Self {
            self.responses.insert(url.to_string(), Ok(body.to_vec()));
            self
        }

        pub fn failing(mut self, url: &str) -> Self {
            let err = UpdateError::Network("timed out".into());
            self.responses.insert(url.to_string(), Err(err));
            self
        }
    }

    impl HttpGet for FakeHttp {
        fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, UpdateError> {
            self.requests
                .borrow_mut()
                .push((url.to_string(), max_bytes));
            self.responses
                .get(url)
                .cloned()
                .unwrap_or_else(|| Err(UpdateError::Network(format!("no route to {url}"))))
        }
    }
}
