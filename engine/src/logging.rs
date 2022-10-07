// ==== Logging key constants ====
pub const COMPONENT_KEY: &str = "component";
pub const CEREMONY_ID_KEY: &str = "ceremony_id";
pub const CEREMONY_TYPE_KEY: &str = "ceremony_type";
pub const REPORTED_PARTIES_KEY: &str = "reported_parties";

// ==== Logging Error/Warning Tag constants ====
pub const REQUEST_TO_SIGN_IGNORED: &str = "E0";
pub const SIGNING_CEREMONY_FAILED: &str = "E2";
pub const KEYGEN_REQUEST_IGNORED: &str = "E3";
pub const KEYGEN_REQUEST_EXPIRED: &str = "E4";
pub const KEYGEN_CEREMONY_FAILED: &str = "E5";
pub const KEYGEN_REJECTED_INCOMPATIBLE: &str = "E6";
// pub const CEREMONY_REQUEST_IGNORED: &str = "E7"; // No longer used
pub const UNAUTHORIZED_SIGNING_ABORTED: &str = "E8";
pub const UNAUTHORIZED_KEYGEN_ABORTED: &str = "E9";

// ==== Logging Eth Witnesser constants ====
pub const ETH_HTTP_STREAM_YIELDED: &str = "eth-witnesser-http-yielded";
pub const ETH_WS_STREAM_YIELDED: &str = "eth-witnesser-ws-yielded";
pub const ETH_STREAM_BEHIND: &str = "eth-stream-behind";

// ==== Logging Trace/Debug Tag constants ====
pub const LOG_ACCOUNT_STATE: &str = "T1";

pub mod utils {
    /// Async slog channel size
    const ASYNC_SLOG_CHANNEL_SIZE: usize = 1024;

    use super::COMPONENT_KEY;
    const KV_LIST_INDENT: &str = "    \x1b[0;34m|\x1b[0m";
    const LOCATION_INDENT: &str = "    \x1b[0;34m-->\x1b[0m";

    use chrono;
    use slog::{o, Drain, Fuse, Key, Level, OwnedKVList, Record, Serializer, KV};
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::{fmt, result};

    fn print_readable_log(record: &Record) {
        // Color code with level
        let level_color = match record.level() {
            Level::Error | Level::Critical => "[0;31m",
            Level::Warning => "[0;33m",
            Level::Info => "[0;36m",
            Level::Debug => "[0;32m",
            Level::Trace => "[0;35m",
        };

        // Print the readable log
        println!(
            "\x1b{}[{}]\x1b[0m {} {}",
            level_color,
            record.level(),
            record.msg(),
            // Only show the tag if its not empty
            if !record.tag().is_empty() {
                format!("([{}], {})", record.tag(), record.module())
            } else {
                format!("({})", record.module())
            }
        );

        // Print the location of the log call if its a Warning or above
        if record.level().is_at_least(Level::Warning) {
            println!("{} {}:{}", LOCATION_INDENT, record.file(), record.line());
        }
    }

    struct PrintlnDrain;

    impl Drain for PrintlnDrain {
        type Ok = ();
        type Err = ();

        fn log(&self, record: &Record, _: &OwnedKVList) -> result::Result<Self::Ok, Self::Err> {
            print_readable_log(record);
            Ok(())
        }
    }

    struct PrintlnSerializer;

    impl Serializer for PrintlnSerializer {
        fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> Result<(), slog::Error> {
            if key != COMPONENT_KEY {
                println!("{} {} = {}", KV_LIST_INDENT, key, val);
            }
            Ok(())
        }
    }

    pub struct PrintlnDrainVerbose;

    impl Drain for PrintlnDrainVerbose {
        type Ok = ();
        type Err = ();

        fn log(
            &self,
            record: &Record,
            values: &OwnedKVList,
        ) -> result::Result<Self::Ok, Self::Err> {
            print_readable_log(record);
            record
                .kv()
                .serialize(record, &mut PrintlnSerializer)
                .unwrap();

            values.serialize(record, &mut PrintlnSerializer).unwrap();
            Ok(())
        }
    }

    /// Prints an easy to read log. eg:
    /// ```sh
    /// [level] <module::module> - <msg>
    /// ```
    pub fn new_cli_logger() -> slog::Logger {
        slog::Logger::root(Fuse(PrintlnDrain), o!())
    }

    /// Prints an easy to read log and the list of key/values. eg:
    /// ```sh
    /// [level] <module::module> - <msg>
    ///     <Key> = <value>
    ///     <Key> = <value>
    /// ```
    pub fn new_cli_logger_verbose() -> slog::Logger {
        slog::Logger::root(Fuse(PrintlnDrainVerbose), o!())
    }

    /// Logger that discards everything, useful when typical logging isn't necessary
    /// or is distracting e.g. in the CLI
    pub fn new_discard_logger() -> slog::Logger {
        slog::Logger::root(slog::Discard, o!())
    }

    /// Creates an async json logger with the 'tag' added as a key (not a key by default)
    /// ```sh
    /// {"msg":"...","level":"trace","ts":"2021-10-21T12:49:22.492673400+11:00","tag":"...", "my_key":"my value"}
    /// ```
    pub fn new_json_logger() -> slog::Logger {
        slog::Logger::root(
            slog_async::Async::new(new_json_drain())
                .chan_size(ASYNC_SLOG_CHANNEL_SIZE)
                .build()
                .fuse(),
            o!(),
        )
    }

    /// Creates an async json logger with the 'tag' added as a key (not a key by default)
    /// Also filters the log using the tags
    pub fn new_json_logger_with_tag_filter(
        tag_whitelist: Vec<String>,
        tag_blacklist: Vec<String>,
    ) -> slog::Logger {
        let drain = RuntimeTagFilter {
            drain: new_json_drain(),
            whitelist: Arc::new(tag_whitelist.into_iter().collect::<HashSet<_>>()),
            blacklist: Arc::new(tag_blacklist.into_iter().collect::<HashSet<_>>()),
        }
        .fuse();
        slog::Logger::root(
            slog_async::Async::new(drain)
                .chan_size(ASYNC_SLOG_CHANNEL_SIZE)
                .build()
                .fuse(),
            o!(),
        )
    }

    /// Creates a custom json drain that includes the tag as a key
    /// and a different level name format
    fn new_json_drain() -> Fuse<slog_json::Json<std::io::Stdout>> {
        slog_json::Json::new(std::io::stdout())
            .add_key_value(slog::o!(
            "ts" => slog::PushFnValue(move |_ : &Record, ser| {
                ser.emit(chrono::Utc::now().to_rfc3339())
            }),
            "level" => slog::FnValue(move |rec : &Record| {
                rec.level().as_str().to_lowercase()
            }),
            "msg" => slog::PushFnValue(move |rec : &Record, ser| {
                ser.emit(rec.msg())
            }),
            "tag" => slog::PushFnValue(move |rec : &slog::Record, ser| {
                ser.emit(rec.tag())
            })))
            .build()
            .fuse()
    }

    pub struct RuntimeTagFilter<D> {
        pub drain: D,
        pub whitelist: Arc<HashSet<String>>,
        pub blacklist: Arc<HashSet<String>>,
    }

    impl<D> Drain for RuntimeTagFilter<D>
    where
        D: Drain,
    {
        type Ok = Option<D::Ok>;
        type Err = Option<D::Err>;

        fn log(
            &self,
            record: &slog::Record,
            values: &slog::OwnedKVList,
        ) -> result::Result<Self::Ok, Self::Err> {
            if !self.blacklist.iter().any(|s| *s == record.tag()) {
                if self.whitelist.iter().any(|s| *s == record.tag()) || self.whitelist.is_empty() {
                    self.drain.log(record, values).map(Some).map_err(Some)
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }
    }

    /// Prints an easy to read log and the list of key/values.
    /// Also filters the log via the tag.
    /// If the `tag_whitelist` is empty, it will allow all except whats on the `tag_blacklist`
    pub fn new_cli_logger_with_tag_filter(
        tag_whitelist: Vec<String>,
        tag_blacklist: Vec<String>,
    ) -> slog::Logger {
        let drain = RuntimeTagFilter {
            drain: PrintlnDrainVerbose,
            whitelist: Arc::new(tag_whitelist.into_iter().collect::<HashSet<_>>()),
            blacklist: Arc::new(tag_blacklist.into_iter().collect::<HashSet<_>>()),
        }
        .fuse();
        slog::Logger::root(slog_async::Async::new(drain).build().fuse(), o!())
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::utils::*;
    use slog::{o, Drain, Fuse, OwnedKVList, Record};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    pub struct TagCache {
        log: Arc<Mutex<Vec<String>>>,
    }

    impl TagCache {
        /// returns true if the given tag was found in the log
        pub fn contains_tag(&self, tag: &str) -> bool {
            self.get_tag_count(tag) > 0
        }

        /// returns the number of times the tag was seen in the log
        pub fn get_tag_count(&self, tag: &str) -> usize {
            self.log
                .lock()
                .expect("Should be able to get lock")
                .iter()
                .filter(|log_tag| *log_tag == tag)
                .count()
        }

        /// Just start again
        pub fn clear(&mut self) {
            *self.log.lock().unwrap() = Vec::new();
        }
    }

    impl Drain for TagCache {
        type Ok = ();
        type Err = ();

        fn log(&self, record: &Record, _: &OwnedKVList) -> Result<Self::Ok, Self::Err> {
            if !record.tag().is_empty() {
                let mut log = self.log.lock().expect("Should be able to get lock");
                log.push(record.tag().to_owned());
            }
            Ok(())
        }
    }

    /// Prints an easy to read log and the list of key/values.
    /// Also creates a tag cache that collects all tags so you can later confirm a log was triggered.
    pub fn new_test_logger_with_tag_cache() -> (slog::Logger, TagCache) {
        let d1 = TagCache::default();
        let d2 = Fuse(PrintlnDrainVerbose);
        (
            slog::Logger::root(slog::Duplicate::new(d1.clone(), d2).fuse(), o!()),
            d1,
        )
    }

    /// Prints an easy to read log and the list of key/values.
    /// Also filters the logs via tags before displaying the log and collecting them in the cache
    /// If the `tag_whitelist` is empty, it will allow all except whats on the `tag_blacklist`
    pub fn new_test_logger_with_tag_cache_and_tag_filter(
        tag_whitelist: Vec<String>,
        tag_blacklist: Vec<String>,
    ) -> (slog::Logger, TagCache) {
        let tc = TagCache::default();
        let tag_whitelist = Arc::new(tag_whitelist.into_iter().collect::<HashSet<_>>());
        let tag_blacklist = Arc::new(tag_blacklist.into_iter().collect::<HashSet<_>>());

        let drain1 = RuntimeTagFilter {
            drain: tc.clone(),
            whitelist: tag_whitelist.clone(),
            blacklist: tag_blacklist.clone(),
        }
        .fuse();

        let drain2 = RuntimeTagFilter {
            drain: PrintlnDrainVerbose,
            whitelist: tag_whitelist,
            blacklist: tag_blacklist,
        }
        .fuse();
        (
            slog::Logger::root(slog::Duplicate::new(drain1, drain2).fuse(), o!()),
            tc,
        )
    }

    /// Prints an easy to read log and the list of key/values.
    pub fn new_test_logger() -> slog::Logger {
        new_cli_logger_verbose()
    }
}

#[cfg(test)]
mod tests {
    use crate::logging::test_utils::*;

    #[test]
    fn test_logging_tags() {
        // Create a logger and tag cache
        let (logger, tag_cache) = new_test_logger_with_tag_cache();
        let logger2 = logger.clone();

        // Print a bunch of stuff with tags
        slog::error!(logger, #"E1234", "Test error");
        slog::error!(logger, #"E1234", "Test error again");
        slog::warn!(logger2, #"2222", "Test warning");

        // Check that tags are collected in the same cache, even from the logger clone
        assert!(tag_cache.contains_tag("E1234"));
        assert_eq!(tag_cache.get_tag_count("E1234"), 2);
        assert!(tag_cache.contains_tag("2222"));
        assert!(!tag_cache.contains_tag("not_tagged"));
    }

    #[test]
    fn test_logging_tag_filter() {
        // Create a logger with a whitelist & blacklist
        let (logger, tag_cache) = new_test_logger_with_tag_cache_and_tag_filter(
            vec!["included".to_owned()],
            vec!["excluded".to_owned()],
        );

        // Print a bunch of stuff with tags
        slog::error!(logger, #"included", "on the whitelist");
        slog::error!(logger, #"not_included", "not on the whitelist");
        slog::info!(logger, "No tag on this");
        slog::warn!(logger, #"excluded", "on the blacklist");

        // Check that it was filtered correctly
        assert!(tag_cache.contains_tag("included"));
        assert!(!tag_cache.contains_tag("not_included"));
        assert!(!tag_cache.contains_tag("excluded"));
    }

    #[test]
    fn test_logging_tag_filter_empty() {
        // Create a logger with an empty whitelist
        let (logger, tag_cache) =
            new_test_logger_with_tag_cache_and_tag_filter(vec![], vec!["excluded".to_owned()]);

        // Check that an empty whitelist lets all through except blacklist
        slog::error!(logger, #"not_included", "no whitelist");
        slog::warn!(logger, #"excluded", "on the blacklist");
        assert!(tag_cache.contains_tag("not_included"));
        assert!(!tag_cache.contains_tag("excluded"));
    }
}
