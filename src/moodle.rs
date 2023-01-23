use crate::attendance::Attendance;
use crate::config;
use crate::moodle_extender::MoodleExtender;
use crate::time_trace::TimeTrace;
use anyhow::{anyhow, bail, Context, Result};
use email_address::EmailAddress;
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header::{HeaderValue, COOKIE, LOCATION};
use reqwest::redirect::Policy;
use reqwest::Url;
use reqwest_tracing::TracingMiddleware;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::num::NonZeroU32;
use std::time::Duration;
use tracing::{info, instrument};

static EMAIL_EXTRACT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<dt>Email address</dt><dd><a href="([^"]+)">"#).unwrap());
static SESSION_EXTRACT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""sesskey":"([^"]+)""#).unwrap());

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoodleUser {
    session: String,
    email: String,
}

impl Display for MoodleUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.email)
    }
}

pub struct Moodle {
    extender: MoodleExtender,
    reqwest: reqwest_middleware::ClientWithMiddleware,
    base_url: Url,
    rate_limiter: RateLimiter<NotKeyed, InMemoryState, DefaultClock>,
}

#[derive(Serialize)]
struct AjaxPayload<T> {
    index: u32,
    methodname: String,
    args: T,
}

#[derive(Debug)]
#[allow(dead_code)]
struct AjaxError {
    pub text: String,
    pub code: String,
}

impl Display for AjaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for AjaxError {}

#[derive(Debug)]
enum AjaxResult<T: Deserialize<'static>> {
    Ok(T),
    SessionDead,
    Error(AjaxError),
}

#[derive(Debug)]
enum SessionProbeResult {
    Invalid,
    Valid { email: String, csrf_session: String },
}

impl Moodle {
    pub async fn new(config: &config::Moodle, extender: MoodleExtender) -> Result<Self> {
        let period = Duration::from_millis(1000 * 60 / config.rpm as u64);

        let quota = Quota::with_period(period)
            .context("Period is invalid")?
            .allow_burst(NonZeroU32::new(config.max_burst).context("Burst is invalid")?);

        let rate_limiter = governor::RateLimiter::direct(quota);

        Ok(Moodle {
            extender,
            reqwest: reqwest_middleware::ClientBuilder::new(
                reqwest::ClientBuilder::new()
                    .user_agent(config.user_agent.clone())
                    .redirect(Policy::none())
                    .build()?,
            )
            .with(TracingMiddleware::<TimeTrace>::new())
            .build(),
            base_url: config.base_url.clone(),
            rate_limiter,
        })
    }

    pub async fn make_user(&self, session: String) -> Result<Option<MoodleUser>> {
        let email = self
            .extender
            .extend_session(&session)
            .await
            .context("Extending session")?;

        Ok(email.map(|email| MoodleUser { session, email }))
    }

    #[instrument(skip_all)]
    async fn check_session(&self, moodle_session: &str) -> Result<SessionProbeResult> {
        self.rate_limiter.until_ready().await;

        let url = self.base_url.join("/user/profile.php")?;

        let resp = self
            .reqwest
            .get(url)
            .header(
                COOKIE,
                HeaderValue::from_str(&format!("MoodleSession={}", moodle_session))?,
            )
            .send()
            .await?;
        if resp.status().is_redirection() {
            info!(
                "Moodle redirected using status {} to {:?}; sessions is likely invalid",
                resp.status(),
                resp.headers().get(LOCATION)
            );
            return Ok(SessionProbeResult::Invalid);
        }

        let body = resp.text().await?;
        let encoded_email = EMAIL_EXTRACT_REGEX
            .captures(&body)
            .context("Could not find email on the profile page")?
            .get(1)
            .unwrap()
            .as_str();

        let email = urlencoding::decode(encoded_email).context("Decoding email")?;
        let email = html_escape::decode_html_entities(&email);
        let email = email
            .strip_prefix("mailto:")
            .context("Stripping mailto prefix")?;

        if !EmailAddress::is_valid(email) {
            return Err(anyhow!(
                "Extracted email address {}, but it seems to be invalid",
                email
            ));
        }

        info!("Session seems to be valid; email = {}", email);

        let sesskey = SESSION_EXTRACT_REGEX
            .captures(&body)
            .context("Could not find sesskey on the profile page")?
            .get(1)
            .unwrap()
            .as_str();

        Ok(SessionProbeResult::Valid {
            email: email.to_string(),
            csrf_session: sesskey.to_string(),
        })
    }

    #[instrument(skip_all, fields(name = format!("ajax {}", method_name)))]
    async fn ajax<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        moodle_session: &str,
        csrf_session: &str,
        method_name: &str,
        args: T,
    ) -> Result<AjaxResult<R>> {
        self.rate_limiter.until_ready().await;

        let url = self
            .base_url
            .join(&format!("/lib/ajax/service.php?sesskey={}", csrf_session))?;

        let resp = self
            .reqwest
            .post(url)
            .header(
                COOKIE,
                HeaderValue::from_str(&format!("MoodleSession={}", moodle_session))?,
            )
            .json(&[AjaxPayload::<T> {
                index: 0,
                methodname: method_name.to_string(),
                args,
            }])
            .send()
            .await?;

        let resp = resp.text().await.context("Reading body as string")?;

        let resp: [serde_json::Map<String, serde_json::Value>; 1] =
            serde_json::from_str(&resp).context("Parsing body as untyped JSON")?;
        let [resp] = resp;
        let error = resp
            .get("error")
            .ok_or_else(|| anyhow!("Missing \"error\" field in response"))?;
        if let Some(err) = error.as_str() {
            let errcode = resp
                .get("errorcode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing \"errorcode\" field in response or wrong type"))?;
            return Ok(AjaxResult::Error(AjaxError {
                text: err.to_string(),
                code: errcode.to_string(),
            }));
        } else if let Some(true) = error.as_bool() {
            let exception = resp
                .get("exception")
                .and_then(|v| v.as_object())
                .ok_or_else(|| anyhow!("Missing \"exception\" field in response or wrong type"))?;
            let message = exception
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing \"message\" field in exception or wrong type"))?;
            let errorcode = exception
                .get("errorcode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing \"errorcode\" field in exception or wrong type"))?;

            if errorcode == "servicerequireslogin" {
                return Ok(AjaxResult::SessionDead);
            }

            return Ok(AjaxResult::Error(AjaxError {
                text: message.to_string(),
                code: errorcode.to_string(),
            }));
        }

        let data = resp
            .get("data")
            .ok_or_else(|| anyhow!("Missing \"data\" field in response"))?;

        Ok(AjaxResult::Ok(
            serde_json::from_value(data.clone())
                .context("Parsing response \"data\" field as typed result")?,
        ))
    }

    pub async fn check_user(&self, user: &MoodleUser) -> Result<bool> {
        Ok(match self.check_session(&user.session).await? {
            SessionProbeResult::Valid { .. } => true,
            SessionProbeResult::Invalid => false,
        })
    }

    pub async fn mark_attendance(&self, user: &MoodleUser, attendance: &Attendance) -> Result<()> {
        let (email, csrf) = match self.check_session(&user.session).await? {
            SessionProbeResult::Valid {
                email,
                csrf_session,
            } => (email, csrf_session),
            SessionProbeResult::Invalid => bail!("Invalid session"),
        };

        info!("Marking attendance for {}...", email);

        Ok(())
    }
}
