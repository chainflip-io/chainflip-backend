use std::{fmt::Display, str::FromStr};

mod ethereum;
pub use ethereum::*;

mod oxen;
pub use oxen::*;

mod bitcoin;
pub use bitcoin::*;

/// A generic address
pub trait Address: Display + FromStr {}
