use crate::config;
use crate::reqwest_span_backend::MoodleExtenderSpanBackend;
use anyhow::Result;
use reqwest_tracing::TracingMiddleware;
use serde::{Deserialize, Serialize};
use tracing::{instrument, trace};
use url::Url;

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
                .with(TracingMiddleware::<MoodleExtenderSpanBackend>::new())
                .build(),
            base_url: config.base_url.clone(),
        })
    }

    #[instrument(skip_all, err, ret)]
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
