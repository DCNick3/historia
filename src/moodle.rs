use crate::attendance::Attendance;
use crate::config;
use crate::moodle_extender::MoodleExtender;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use thiserror::Error;

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

#[derive(Debug, Error)]
pub enum MoodleError {
    #[error("Provided session is invalid")]
    SessionInvalid,
    #[error("Http error: {0}")]
    Http(#[from] reqwest_middleware::Error),
    #[error("Error extending session: {0}")]
    Extender(anyhow::Error),
}

pub struct Moodle {
    extender: MoodleExtender,
}

impl Moodle {
    pub async fn new(_config: &config::Moodle, extender: MoodleExtender) -> anyhow::Result<Self> {
        Ok(Moodle { extender })
    }

    pub async fn make_user(&self, session: String) -> Result<MoodleUser, MoodleError> {
        let email = self
            .extender
            .extend_session(&session)
            .await
            .map_err(MoodleError::Extender)?;

        match email {
            Some(email) => Ok(MoodleUser { session, email }),
            None => Err(MoodleError::SessionInvalid),
        }
    }

    pub async fn check_user(&self, _user: &MoodleUser) -> Result<bool, ()> {
        // check if session is expired
        Ok(false)
    }

    pub async fn mark_attendance(
        &self,
        _user: &MoodleUser,
        _attendance: &Attendance,
    ) -> Result<(), ()> {
        Ok(())
    }
}
