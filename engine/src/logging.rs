// ==== Logging key constants ====
pub const COMPONENT_KEY: &str = "component";

pub const SIGNING_SUB_COMPONENT: &str = "signing-sub-component";

#[cfg(test)]
pub mod test_utils {

    use slog::{o, Drain};

    pub fn create_test_logger() -> slog::Logger {
        let drain = slog_json::Json::new(std::io::stdout())
            .add_default_keys()
            .build()
            .fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let root = slog::Logger::root(drain, o!());
        return root;
    }
}
