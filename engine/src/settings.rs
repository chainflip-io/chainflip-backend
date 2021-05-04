use config::{Config, ConfigError, File};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct MessageQueue {
    pub hostname: String,
    pub port: u32,
}

#[derive(Debug, Deserialize)]
pub struct Subxt {
    pub hostname: String,
    pub port: u32,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub message_queue: MessageQueue,
    pub subxt: Subxt,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let mut s = Config::new();

        // Start off by merging in the "default" configuration file
        s.merge(File::with_name("config/default.toml"))?;

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_into()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn init_config() {
        let settings = Settings::new();
        assert!(settings.is_ok());

        let settings = settings.unwrap();
        assert_eq!(settings.message_queue.hostname, "localhost");
    }
}
