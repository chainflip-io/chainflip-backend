#![warn(missing_docs)]

//! Main blockswap library shared between Vault and Quoter

/// Code that is commonly used by other modules
pub mod common;
/// Logging set up
pub mod logging;
/// Quoter implementation
pub mod quoter;
/// Side Chain implementation
pub mod side_chain;
/// Various transaction types commonly used by other modules
pub mod transactions;
/// Helper functions (including helper functions for tests)
pub mod utils;
/// Vault implementation
pub mod vault;

#[macro_use]
extern crate log;

#[macro_use]
extern crate lazy_static;

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
