#![warn(missing_docs)]

//! Main chainflip library shared between Vault and Quoter

/// Code that is commonly used by other modules
pub mod common;
/// Local store, stores the full transaction data, the data that's not stored on the substrate node
pub mod local_store;
/// Logging set up
pub mod logging;
/// Quoter implementation
pub mod quoter;
/// Helper functions (including helper functions for tests)
pub mod utils;
/// Vault implementation
pub mod vault;

/// Constants in the application
pub mod constants;

#[macro_use]
extern crate log;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate async_trait;

extern crate chainflip_common;

/// Temporary funciton to demostrate how to use
/// unit/integration tests
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Unit test sample
#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
    }
}
