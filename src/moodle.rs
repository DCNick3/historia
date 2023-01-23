use crate::attendance::Attendance;
use crate::config;
use crate::moodle_extender::MoodleExtender;
use crate::time_trace::TimeTrace;
use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate};
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
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::num::NonZeroU32;
use std::time::Duration;
use tracing::{debug, info, instrument};

static EMAIL_EXTRACT_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"<dt>(?:Email address|Адрес электронной почты)</dt><dd><a href="([^"]+)">"#)
        .unwrap()
});
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
pub enum SessionProbeResult {
    Invalid,
    Valid { email: String, csrf_session: String },
}

#[derive(Debug)]
pub struct AttendanceSession {
    pub id: u32,
    pub date: NaiveDate,
}

impl AttendanceSession {
    pub fn matches(&self, attendance: &Attendance) -> bool {
        self.date.day() == attendance.day as u32 && self.date.month() == attendance.month as u32
    }
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

    #[instrument(skip_all)]
    pub async fn make_user(&self, session: String) -> Result<Option<MoodleUser>> {
        let email = self
            .extender
            .extend_session(&session)
            .await
            .context("Extending session")?;

        Ok(email.map(|email| MoodleUser { session, email }))
    }

    #[instrument(skip_all, fields(user = %user.email))]
    pub async fn check_user(&self, user: &MoodleUser) -> Result<SessionProbeResult> {
        self.rate_limiter.until_ready().await;

        let url = self.base_url.join("/user/profile.php")?;

        let resp = self
            .reqwest
            .get(url)
            .header(
                COOKIE,
                HeaderValue::from_str(&format!("MoodleSession={}", user.session))?,
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

    #[instrument(skip_all, fields(%activity_id, user = %user.email))]
    pub async fn get_attendance_sessions(
        &self,
        activity_id: u32,
        user: &MoodleUser,
    ) -> Result<Vec<AttendanceSession>> {
        self.rate_limiter.until_ready().await;

        let url = self.make_attendance_url(activity_id)?;

        let resp = self
            .reqwest
            .get(url)
            .header(
                COOKIE,
                HeaderValue::from_str(&format!("MoodleSession={}", user.session))?,
            )
            .send()
            .await?
            .error_for_status()?;

        static TABLE_SELECTOR: Lazy<Selector> = Lazy::new(|| {
            Selector::parse("table.generaltable.attwidth.boxaligncenter > tbody").unwrap()
        });
        static DATE_SELECTOR: Lazy<Selector> =
            Lazy::new(|| Selector::parse("td:nth-of-type(1)").unwrap());
        static LINK_SELECTOR: Lazy<Selector> =
            Lazy::new(|| Selector::parse("td:nth-of-type(3) > a").unwrap());
        static DATE_FORMATS: [&str; 2] = [
            // 23.01.23 (Mon)
            "%d.%m.%y (%a)",
            // Mon 23 Jan 2023
            "%a %d %b %Y",
        ];

        let resp = Html::parse_document(&resp.text().await?);
        let table = resp
            .select(&TABLE_SELECTOR)
            .next()
            .context("Could not find attendance table")?;

        let mut result = Vec::new();
        for session in table.children() {
            debug!("Session node: {:?}", session.value());

            // skip non-element nodes
            let Some(session) = ElementRef::wrap(session) else { continue };
            let date = session
                .select(&DATE_SELECTOR)
                .next()
                .ok_or_else(|| anyhow!("Could not find date in session node"))
                .and_then(|v| {
                    v.text()
                        .next()
                        .ok_or_else(|| anyhow!("Could not find date in session node"))
                })?
                .trim();
            let date = DATE_FORMATS
                .into_iter()
                .map(|fmt| NaiveDate::parse_from_str(&date, fmt).context("Parsing date"))
                .fold(Err(anyhow!("")), |acc, res| acc.or(res))?;

            let Some(link) = session
                .select(&LINK_SELECTOR)
                .next()
            else {
                debug!("Skipping session node, as it's missing a link. Probably a closed session or smth");
                continue
            };

            let link = link
                .value()
                .attr("href")
                .context("Could not find link href")?;
            let link = Url::parse(link).context("Could not parse link")?;
            let id = link
                .query_pairs()
                .find(|(k, _)| k == "sessid")
                .map(|(_, v)| v)
                .context("Could not find sessid in link")?
                .parse::<u32>()
                .context("Parsing id")?;

            result.push(AttendanceSession { id, date });
        }

        Ok(result)
    }

    #[instrument(skip_all, fields(%session_id, %password, user = %user.email))]
    pub async fn mark_attendance_session(
        &self,
        user: &MoodleUser,
        csrf_session: &str,
        session_id: u32,
        password: &str,
    ) -> Result<()> {
        self.rate_limiter.until_ready().await;

        let url = self.base_url.join("/mod/attendance/attendance.php")?;

        #[derive(Serialize)]
        #[allow(non_snake_case)]
        struct Body<'a> {
            sesskey: &'a str,
            sessid: u32,
            _qf__mod_attendance_form_studentattendance: i32,
            mform_isexpanded_id_session: i32,
            studentpassword: &'a str,
            submitbutton: &'a str,
            status: u32,
        }

        let resp = self
            .reqwest
            .post(url)
            .header(
                COOKIE,
                HeaderValue::from_str(&format!("MoodleSession={}", user.session))?,
            )
            .form(&Body {
                sesskey: csrf_session,
                sessid: session_id,
                _qf__mod_attendance_form_studentattendance: 1,
                mform_isexpanded_id_session: 1,
                studentpassword: password,
                submitbutton: "Save changes",
                status: 1371, // magic number
            })
            .send()
            .await?
            .error_for_status()?;

        if !resp.status().is_redirection() {
            bail!(
                "Unexpected response status: {} (expected a redirect). Invalid status?",
                resp.status()
            );
        }

        let location = resp
            .headers()
            .get(LOCATION)
            .ok_or_else(|| anyhow!("Missing header"))
            .and_then(|v| v.to_str().context("Header to str"))
            .and_then(|v| Url::parse(v).context("Parse as Url"))?;

        match location.path() {
            "/mod/attendance/view.php" => Ok(()),
            "/mod/attendance/attendance.php" => {
                // TODO: try to follow and extract the error
                bail!("Moodle redirected to the same page, probably some error occurred")
            }
            path => bail!("Unknown redirect path: {}", path),
        }
    }

    pub fn make_attendance_url(&self, activity_id: u32) -> Result<Url> {
        Ok(self
            .base_url
            .join(&format!("/mod/attendance/view.php?id={}", activity_id))
            .context("Making attendance URL")?)
    }

    pub fn make_session_url(&self, session_id: u32) -> Result<Url> {
        Ok(self
            .base_url
            .join(&format!(
                "/mod/attendance/attendance.php?sessid={}",
                session_id
            ))
            .context("Making session URL")?)
    }
}
