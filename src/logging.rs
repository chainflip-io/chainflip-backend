use log::LevelFilter;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::rolling_file::RollingFileAppender;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::filter::threshold::ThresholdFilter;

use log4rs::append::rolling_file::policy::compound;

/// Initialize the logging framework. The logger will write to the file `base_name`.
/// Logging events will be printed to the console according to `console_level` filter.
pub fn init(base_name: &str, console_level: Option<LevelFilter>) {
    // Print to the console at Info in debug builds (at Warn level in release builds)
    let default_level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    let console_level = console_level.unwrap_or(default_level);

    let file_level = LevelFilter::Debug;

    let encoder = Box::new(PatternEncoder::new("{h({l})} {m}{n}"));
    let stdout = ConsoleAppender::builder().encoder(encoder).build();
    let filter = Box::new(ThresholdFilter::new(console_level));
    let stdout_appender = Appender::builder()
        .filter(filter)
        .build("stdout", Box::new(stdout));

    // Rotate log files every ~50MB keeping 1 archived
    let size_trigger = compound::trigger::size::SizeTrigger::new(50_000_000);
    let roller = compound::roll::fixed_window::FixedWindowRoller::builder()
        .build(&format!("{}-archive.{{}}.log", &base_name), 1)
        .unwrap();
    let roll_policy = compound::CompoundPolicy::new(Box::new(size_trigger), Box::new(roller));

    // Print to the file at Info level
    let file_appender = RollingFileAppender::builder()
        .build(&format!("{}.log", &base_name), Box::new(roll_policy))
        .unwrap();
    let file_appender = Appender::builder().build("file", Box::new(file_appender));

    let root = Root::builder()
        .appender("stdout")
        .appender("file")
        .build(file_level);

    let config = Config::builder()
        .appender(stdout_appender)
        .appender(file_appender)
        .build(root)
        .unwrap();

    let _handle = log4rs::init_config(config).expect("Error initialising log configuration");
}
