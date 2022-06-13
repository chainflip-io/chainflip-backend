// Add a special cool method for adding line numbers
// Ripped from: https://github.com/dtolnay/anyhow/issues/22

macro_rules! here {
    () => {
        concat!("at ", file!(), " line ", line!(), " column ", column!())
    };
}
