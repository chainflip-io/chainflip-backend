use std::sync::Once;

static INIT: Once = Once::new();

/// Initializes the logger and does only once
/// (doing otherwise would result in error)
pub fn init() {
    INIT.call_once(|| {
        env_logger::builder()
            .format_timestamp(None)
            .format_module_path(false)
            .init();
    })
}
