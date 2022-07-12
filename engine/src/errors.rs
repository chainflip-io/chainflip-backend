// Add a special cool method for adding line numbers
// Ripped from: https://github.com/dtolnay/anyhow/issues/22

macro_rules! here {
    () => {
        concat!("at ", file!(), " line ", line!(), " column ", column!())
    };
}

macro_rules! context {
    ($e:expr) => {{
        use anyhow::Context;

        $e.with_context(|| format!("Error: '{}' failed {}", stringify!($e), here!()))
    }};
}
