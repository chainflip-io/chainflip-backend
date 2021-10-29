// ==== Logging key constants ====
pub const COMPONENT_KEY: &str = "component";
pub const CEREMONY_ID_KEY: &str = "ceremony_id";

// ==== Logging Error/Warning Tag constants ====
pub const REQUEST_TO_SIGN_IGNORED: &str = "0";

// ==== Logging Trace Tag constants ====
pub const PROCESS_SIGNING_DATA: &str = "T0";

pub mod utils {

    use super::COMPONENT_KEY;
    const KV_LIST_INDENT: &str = "    \x1b[0;34m|\x1b[0m";
    const LOCATION_INDENT: &str = "    \x1b[0;34m-->\x1b[0m";

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
            "\x1b{}[{}]\x1b[0m {} {} - {}",
            level_color,
            record.level(),
            record.tag(),
            record.module(),
            record.msg()
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
    pub fn create_cli_logger() -> slog::Logger {
        slog::Logger::root(Fuse(PrintlnDrain), o!())
    }

    /// Prints an easy to read log and the list of key/values. eg:
    /// ```sh
    /// [level] <module::module> - <msg>
    ///     <Key> = <value>
    ///     <Key> = <value>
    /// ```
    pub fn create_cli_logger_verbose() -> slog::Logger {
        slog::Logger::root(Fuse(PrintlnDrainVerbose), o!())
    }

    /// Creates an async json logger with the 'tag' added as a key (not a key by default)
    /// ```sh
    /// {"msg":"...","level":"TRCE","ts":"2021-10-21T12:49:22.492673400+11:00","tag":"...", "my_key":"my value"}
    /// ```
    pub fn create_json_logger() -> slog::Logger {
        slog::Logger::root(
            slog_async::Async::new(create_json_drain()).build().fuse(),
            o!(),
        )
    }

    /// Creates an async json logger with the 'tag' added as a key (not a key by default)
    /// Also filters the log using the tags
    pub fn create_json_logger_with_tag_filter(
        tag_whitelist: Vec<String>,
        tag_blacklist: Vec<String>,
    ) -> slog::Logger {
        let drain = RuntimeTagFilter {
            drain: create_json_drain(),
            whitelist: Arc::new(tag_whitelist.iter().cloned().collect::<HashSet<_>>()),
            blacklist: Arc::new(tag_blacklist.iter().cloned().collect::<HashSet<_>>()),
        }
        .fuse();
        slog::Logger::root(slog_async::Async::new(drain).build().fuse(), o!())
    }

    /// Creates a custom json drain that includes the tag as a key
    fn create_json_drain() -> Fuse<slog_json::Json<std::io::Stdout>> {
        slog_json::Json::new(std::io::stdout())
            .add_default_keys()
            .add_key_value(
                slog::o!("tag" => slog::PushFnValue(move |rec : &slog::Record, ser| {
                    ser.emit(rec.tag())
                })),
            )
            .build()
            .fuse()
    }

    pub struct RuntimeTagFilter<D> {
        pub drain: D,
        // pub whitelist: Arc<Vec<String>>,
        // pub blacklist: Arc<Vec<String>>,
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
            if self.blacklist.iter().find(|s| *s == record.tag()).is_none() {
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
    pub fn create_cli_logger_with_tag_filter(
        tag_whitelist: Vec<String>,
        tag_blacklist: Vec<String>,
    ) -> slog::Logger {
        let drain = RuntimeTagFilter {
            drain: PrintlnDrainVerbose,
            whitelist: Arc::new(tag_whitelist.iter().cloned().collect::<HashSet<_>>()),
            blacklist: Arc::new(tag_blacklist.iter().cloned().collect::<HashSet<_>>()),
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
    use std::iter::FromIterator;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct TagCache {
        log: Arc<Mutex<Vec<String>>>,
    }

    impl TagCache {
        pub fn new() -> Self {
            let log = Arc::new(Mutex::new(vec![]));
            Self { log }
        }

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

        /// clear the tag cache
        pub fn clear(&mut self) {
            self.log.lock().expect("Should be able to get lock").clear();
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
    pub fn create_test_logger_with_tag_cache() -> (slog::Logger, TagCache) {
        let d1 = TagCache::new();
        let d2 = Fuse(PrintlnDrainVerbose);
        (
            slog::Logger::root(slog::Duplicate::new(d1.clone(), d2).fuse(), o!()),
            d1,
        )
    }

    /// Prints an easy to read log and the list of key/values.
    /// Also filters the logs via tags before displaying the log and collecting them in the cache
    /// If the `tag_whitelist` is empty, it will allow all except whats on the `tag_blacklist`
    pub fn create_test_logger_with_tag_cache_and_tag_filter(
        tag_whitelist: Vec<String>,
        tag_blacklist: Vec<String>,
    ) -> (slog::Logger, TagCache) {
        let tc = TagCache::new();
        let tag_whitelist = Arc::new(tag_whitelist.iter().cloned().collect::<HashSet<_>>());
        let tag_blacklist = Arc::new(tag_blacklist.iter().cloned().collect::<HashSet<_>>());

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
    pub fn create_test_logger() -> slog::Logger {
        create_cli_logger_verbose()
    }
}

#[test]
fn test_logging_tags() {
    use super::logging::test_utils::*;

    // Create a logger and tag cache
    let (logger, mut tag_cache) = create_test_logger_with_tag_cache();
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

    // Check that clearing the cache works
    tag_cache.clear();
    assert!(!tag_cache.contains_tag("E1234"));
}

#[test]
fn test_logging_tag_filter() {
    use super::logging::test_utils::*;

    // Create a logger and whitelist/blacklist
    let (logger, tag_cache) = create_test_logger_with_tag_cache_and_tag_filter(
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

    // Clear the whitelist and tag cache
    let (logger, tag_cache) =
        create_test_logger_with_tag_cache_and_tag_filter(vec![], vec!["excluded".to_owned()]);

    // Test that an empty whitelist lets all through except blacklist
    slog::error!(logger, #"not_included", "no more whitelist");
    slog::warn!(logger, #"excluded", "on the blacklist");
    assert!(tag_cache.contains_tag("not_included"));
    assert!(!tag_cache.contains_tag("excluded"));
}
