use crate::attendance::Attendance;
use crate::config;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

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

pub struct Moodle {}

impl Moodle {
    pub async fn new(_config: &config::Moodle) -> anyhow::Result<Self> {
        Ok(Moodle {})
    }

    pub async fn make_user(&self, session: String) -> Result<MoodleUser, ()> {
        // TODO: send email to moodle session extender

        Ok(MoodleUser {
            session,
            email: "test@innopolis.university".to_string(),
        })
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
