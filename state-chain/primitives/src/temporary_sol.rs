/// Define `Solana` type. Derive what is necessary.
/// Do not add a variant into ForeignChain.

pub mod chain {
	use codec::{Decode, Encode};
	use frame_support::pallet_prelude::RuntimeDebug;
	use scale_info::TypeInfo;

	use crate::chains::{ForeignChain, Get};

	#[derive(Copy, Clone, RuntimeDebug, Default, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub struct Solana;

	impl AsRef<ForeignChain> for Solana {
		fn as_ref(&self) -> &ForeignChain {
			unimplemented!()
		}
	}

	impl Get<ForeignChain> for Solana {
		fn get() -> ForeignChain {
			unimplemented!()
		}
	}

	impl From<Solana> for ForeignChain {
		fn from(_: Solana) -> ForeignChain {
			unimplemented!()
		}
	}
}

pub mod asset {
	use codec::{Decode, Encode, MaxEncodedLen};
	use scale_info::TypeInfo;

	use crate::chains::{
		assets::{any, AssetError},
		ForeignChain,
	};

	pub type Chain = super::chain::Solana;

	#[derive(
		Copy,
		Clone,
		Debug,
		PartialEq,
		Eq,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
		Hash,
		serde::Serialize,
		serde::Deserialize,
	)]
	pub enum Asset {
		Sol,
	}

	pub const GAS_ASSET: Asset = Asset::Sol;

	impl From<Asset> for any::Asset {
		fn from(asset: Asset) -> Self {
			match asset {
				Asset::Sol => any::Asset::Sol,
			}
		}
	}

	impl AsRef<any::Asset> for Asset {
		fn as_ref(&self) -> &any::Asset {
			match self {
				Asset::Sol => &any::Asset::Sol,
			}
		}
	}

	impl TryFrom<any::Asset> for Asset {
		type Error = AssetError;

		fn try_from(asset: any::Asset) -> Result<Self, Self::Error> {
			match asset {
				any::Asset::Sol => Ok(Asset::Sol),
				_ => Err(AssetError::Unsupported),
			}
		}
	}

	impl From<Asset> for ForeignChain {
		fn from(_asset: Asset) -> Self {
			From::from(super::chain::Solana)
		}
	}
}
