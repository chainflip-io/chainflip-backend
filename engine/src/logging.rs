// ==== Logging key constants ====
pub const COMPONENT_KEY: &str = "component";
pub const CEREMONY_ID_KEY: &str = "ceremony_id";

pub mod utils {
    use super::COMPONENT_KEY;
    const KV_LIST_INDENT: &str = "    ";

    use slog::{o, Drain, Fuse, Key, Level, OwnedKVList, Record, Serializer, KV};
    use std::{fmt, result};

    fn print_readable_log(record: &Record) {
        let level_color = match record.level() {
            Level::Error | Level::Critical => "[0;31m",
            Level::Warning => "[0;33m",
            Level::Info => "[0;36m",
            Level::Debug => "[0;32m",
            Level::Trace => "[0;35m",
        };
        println!(
            "\x1b{}[{}]\x1b[0m {} - {}",
            level_color,
            record.level(),
            record.module(),
            record.msg()
        );
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

    pub struct PrintlnSerializer;

    impl Serializer for PrintlnSerializer {
        fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> Result<(), slog::Error> {
            if key != COMPONENT_KEY {
                println!("{}{} = {}", KV_LIST_INDENT, key, val);
            }
            Ok(())
        }
    }
    struct PrintlnDrainVerbose;

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
}

#[cfg(test)]
pub mod test_utils {
    use super::utils::*;

    /// Creates a verbose CLI logger that is easy to read.
    pub fn new_test_logger() -> slog::Logger {
        new_cli_logger_verbose()
    }
}
