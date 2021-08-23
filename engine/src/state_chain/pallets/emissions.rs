use codec::FullCodec;
use frame_support::pallet_prelude::{MaybeSerializeDeserialize, Member};
use sp_runtime::traits::{AtLeast32BitUnsigned, UniqueSaturatedFrom};
use substrate_subxt::{module, system::System};

#[module]
pub trait Emissions: System {
    /// The Flip token denomination.
    type FlipBalance: Member
        + FullCodec
        + Default
        + Copy
        + MaybeSerializeDeserialize
        + AtLeast32BitUnsigned
        + UniqueSaturatedFrom<Self::BlockNumber>;
}
