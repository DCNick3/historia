use std::fmt::Display;

#[derive(Debug)]
pub struct Attendance {
    pub day: u8,
    pub month: u8,
    pub password: String,
}

impl Display for Attendance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}: {}", self.day, self.month, self.password)
    }
}
