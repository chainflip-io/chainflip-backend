// ==== Logging key constants ====
pub const COMPONENT_KEY: &str = "component";

pub mod utils {
    use slog::{o, Drain, Fuse, Level, OwnedKVList, Record};
    use std::result;

    struct PrintlnDrain;

    impl Drain for PrintlnDrain {
        type Ok = ();
        type Err = ();

        fn log(&self, record: &Record, _: &OwnedKVList) -> result::Result<Self::Ok, Self::Err> {
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
            Ok(())
        }
    }

    pub fn create_cli_logger() -> slog::Logger {
        slog::Logger::root(Fuse(PrintlnDrain), o!())
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::utils::create_cli_logger;

    pub fn create_test_logger() -> slog::Logger {
        create_cli_logger()
    }
}
