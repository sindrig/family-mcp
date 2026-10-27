use mcp_runtime::{Cancelled, Fail, Server, Surface};
use rmcp::model::JsonObject;
use serde_json::Value;

use crate::{calculator, http, input};

pub struct Rjupa {
    surface: Surface,
    http: http::Http,
}

impl Rjupa {
    pub fn new() -> reqwest::Result<Self> {
        Ok(Self {
            surface: Surface::parse(include_str!("surface.json")),
            http: http::Http::new()?,
        })
    }
}

impl Server for Rjupa {
    type Fail = Fail;
    const NAME: &'static str = "rjupa-mcp";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    fn surface(&self) -> &Surface {
        &self.surface
    }

    async fn call(
        &self,
        name: &str,
        arguments: &JsonObject,
        cancelled: Cancelled,
    ) -> Result<Result<Value, Fail>, String> {
        if name != "get_degree_days" {
            return Err(String::new());
        }
        let now = http::now();
        let query = input::parse(arguments, now)?;
        Ok(tokio::select! {
            output = async {
                let body = self.http.calculate(&query).await?;
                calculator::shape(&body, &query, now, http::now())
            } => output,
            () = cancelled => Err(Fail::Safe("Rjúpa check cancelled. Reminder state was not changed.")),
        })
    }
}
