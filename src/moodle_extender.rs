use crate::config;
use crate::time_trace::TimeTrace;
use anyhow::Result;
use reqwest::Url;
use reqwest_tracing::TracingMiddleware;
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Serialize)]
struct ExtendRequest {
    pub moodle_session: String,
}

#[derive(Deserialize)]
struct ExtendResponse {
    // pub result: bool,
    pub email: Option<String>,
}

pub struct MoodleExtender {
    reqwest: reqwest_middleware::ClientWithMiddleware,
    base_url: Url,
}

impl MoodleExtender {
    pub async fn new(config: &config::MoodleExtender) -> Result<Self> {
        Ok(MoodleExtender {
            reqwest: reqwest_middleware::ClientBuilder::new(reqwest::ClientBuilder::new().build()?)
                .with(TracingMiddleware::<TimeTrace>::new())
                .build(),
            base_url: config.base_url.clone(),
        })
    }

    pub async fn extend_session(&self, session: &str) -> Result<Option<String>> {
        trace!("Extending session {}...", session);

        let rq = ExtendRequest {
            moodle_session: session.to_string(),
        };

        let res = self
            .reqwest
            .post(self.base_url.join("extend-session")?)
            .json(&rq)
            .send()
            .await?;

        let response: ExtendResponse = res.error_for_status()?.json().await?;

        Ok(response.email)
    }
}
