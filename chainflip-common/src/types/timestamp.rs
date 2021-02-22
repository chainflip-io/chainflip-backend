use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

#[cfg(feature = "std")]
use std::time::SystemTime;

/// Unix millisecond timestamp wrapper
#[derive(
    Debug, Copy, Clone, Ord, PartialOrd, PartialEq, Eq, Encode, Decode, Serialize, Deserialize,
)]
pub struct Timestamp(pub u128);

#[cfg(feature = "std")]
impl Timestamp {
    /// Create an instance from `SystemTime`
    pub fn from_system_time(ts: SystemTime) -> Self {
        let millis = ts
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Failed to get unix timestamp")
            .as_millis();
        Timestamp(millis)
    }

    /// Create an instance from current system time
    pub fn now() -> Self {
        Timestamp::from_system_time(SystemTime::now())
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Timestamp {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let ts: u128 = s.parse().map_err(|_| "Timestamp must be valid u128")?;

        Ok(Timestamp(ts))
    }
}
