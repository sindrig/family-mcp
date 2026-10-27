use std::time::Duration;

use chrono::{DateTime, Utc};
use mcp_runtime::Fail;
use serde_json::{Value, json};

use crate::input::Query;

pub const URL: &str = "https://rjupa.irdn.is/api/daggradur";
pub const FAILED: Fail = Fail::Safe(
    "The Rjúpa calculator is unavailable. No reminder state was changed; retry on the next scheduled check.",
);

pub fn now() -> DateTime<Utc> {
    #[cfg(feature = "test-origin")]
    if let Ok(text) = std::env::var("RJUPA_TEST_NOW") {
        return text.parse().expect("RJUPA_TEST_NOW must be UTC");
    }
    Utc::now()
}

pub struct Http(reqwest::Client);

impl Http {
    pub fn new() -> reqwest::Result<Self> {
        let builder = reqwest::Client::builder()
            .user_agent(concat!(
                "rjupa-mcp/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/sindrig/family-mcp)"
            ))
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never());
        #[cfg(feature = "test-origin")]
        let builder = builder.no_proxy();
        Ok(Self(builder.build()?))
    }

    pub async fn calculate(&self, query: &Query) -> Result<Value, Fail> {
        let target = target();
        let response = self.0.post(target)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&json!({"startDate":query.start.to_string(),"endDate":query.end.to_string(),"latitude":query.latitude,"longitude":query.longitude}))
            .send().await.map_err(|_| FAILED)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(FAILED);
        }
        let body = mcp_runtime::read_capped(response, 4 * 1024 * 1024)
            .await
            .map_err(|_| FAILED)?;
        serde_json::from_slice(&body).map_err(|_| Fail::Safe("The Rjúpa calculator returned an unreadable response. Reminder state must remain unchanged."))
    }
}

fn target() -> String {
    #[cfg(feature = "test-origin")]
    {
        // A test build can never accidentally fall through to the live service.
        let origin =
            std::env::var("RJUPA_TEST_ORIGIN").unwrap_or_else(|_| "http://127.0.0.1:9".into());
        let port: u16 = origin
            .strip_prefix("http://127.0.0.1:")
            .and_then(|value| value.parse().ok())
            .filter(|port| *port != 0)
            .expect("RJUPA_TEST_ORIGIN must be an exact loopback origin with a port");
        format!("http://127.0.0.1:{port}/api/daggradur")
    }
    #[cfg(not(feature = "test-origin"))]
    URL.to_owned()
}
