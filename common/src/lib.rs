#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(not(feature = "std"))]
#[macro_use]
extern crate sp_std as std;

#[cfg(not(feature = "std"))]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate lazy_static;

/// Common types
pub mod types;

/// Common utils
pub mod utils;

/// Constants
pub mod constants;

#[cfg(test)]
mod test;
pub mod validation;

/// A convenience module for accessing strings
mod string {
    #[cfg(not(feature = "std"))]
    pub use alloc::string::*;

    #[cfg(feature = "std")]
    pub use std::string::*;
}
