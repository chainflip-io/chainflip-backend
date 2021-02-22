mod swap_quote;

pub use swap_quote::*;

mod deposit_quote;
pub use deposit_quote::*;

mod withdraw_request;
pub use withdraw_request::*;

mod witness;
pub use witness::*;

mod pool_change;
pub use pool_change::*;

mod deposit;
pub use deposit::*;

mod withdraw;
pub use withdraw::*;

mod output;
pub use output::*;

mod output_sent;
pub use output_sent::*;

use super::Network;

/// Defines type of an Event's unique identifier
pub type UniqueId = u64;

/// Trait representing something which can be validated
pub trait Validate {
    type Error;

    /// Check if valid or not
    fn validate(&self, network: Network) -> Result<(), Self::Error>;
}
