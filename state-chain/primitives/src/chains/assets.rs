//! Chainflip Asset types.
//!
//! Assets are defined on a per-chain basis and organised in a module structure so that the type for
//! an asset is scoped to the chain the asset exists on. For example Flip is an Ethereum asset, its
//! type is `eth::Asset` and its value is `eth::Asset::Flip`.
//!
//! The [any] module is special - it collects all asset from all chain. Importantly, each asset
//! belongs to exactly one chain, so it's possible to uniquely convert an asset from another chain
//! to its `any` equivalent:
//!
//! ```
//! use cf_primitives::chains::assets::*;
//!
//! assert_eq!(any::Asset::Flip, any::Asset::from(eth::Asset::Flip));
//! ```
use super::*;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

/// Defines all Assets, and the Chain each asset belongs to.
/// There's a unique 1:1 relationship between an Asset and a Chain.
pub mod any {
	use core::str::FromStr;

	use super::*;
	pub type Chain = AnyChain;

	/// A token or currency that can be swapped natively in the Chainflip AMM.
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
		PartialOrd,
		Ord,
	)]
	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	pub enum Asset {
		// 0 is reservered for particular cross chain messaging scenarios where we want to pass
		// through a message without making a swap.
		Eth = 1,
		Flip = 2,
		Usdc = 3,
		Dot = 4,
		Btc = 5,
	}

	impl TryFrom<u32> for Asset {
		type Error = &'static str;

		fn try_from(n: u32) -> Result<Self, Self::Error> {
			match n {
				x if x == Self::Eth as u32 => Ok(Self::Eth),
				x if x == Self::Flip as u32 => Ok(Self::Flip),
				x if x == Self::Usdc as u32 => Ok(Self::Usdc),
				x if x == Self::Dot as u32 => Ok(Self::Dot),
				x if x == Self::Btc as u32 => Ok(Self::Btc),
				_ => Err("Invalid asset id"),
			}
		}
	}

	impl From<Asset> for ForeignChain {
		fn from(asset: Asset) -> Self {
			match asset {
				Asset::Eth => Self::Ethereum,
				Asset::Flip => Self::Ethereum,
				Asset::Usdc => Self::Ethereum,
				Asset::Dot => Self::Polkadot,
				Asset::Btc => Self::Bitcoin,
			}
		}
	}

	impl FromStr for Asset {
		type Err = &'static str;

		fn from_str(s: &str) -> Result<Self, Self::Err> {
			match s.to_lowercase().as_str() {
				"eth" => Ok(Asset::Eth),
				"flip" => Ok(Asset::Flip),
				"usdc" => Ok(Asset::Usdc),
				"dot" => Ok(Asset::Dot),
				"btc" => Ok(Asset::Btc),
				_ => Err("Unrecognized asset"),
			}
		}
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AssetError {
	Unsupported,
}

/// Defines the assets types for a chain and some useful conversion traits. See the module level
/// docs for more detail.
macro_rules! chain_assets {
	( $mod:ident, $chain:ident, $( $asset:ident ),+ ) => {
		/// Chain-specific assets types.
		pub mod $mod {
			use $crate::chains::*;
			use $crate::chains::assets::*;

			pub type Chain = $chain;

			#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Hash)]
			#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
			pub enum Asset {
				$(
					$asset,
				)+
			}

			impl From<Asset> for any::Asset {
				fn from(asset: Asset) -> Self {
					match asset {
						$(
							Asset::$asset => any::Asset::$asset,
						)+
					}
				}
			}

			impl AsRef<any::Asset> for Asset {
				fn as_ref(&self) -> &any::Asset {
					match self {
						$(
							Asset::$asset => &any::Asset::$asset,
						)+
					}
				}
			}

			impl TryFrom<any::Asset> for Asset {
				type Error = AssetError;

				fn try_from(asset: any::Asset) -> Result<Self, Self::Error> {
					match asset {
						$(
							any::Asset::$asset => Ok(Asset::$asset),
						)+
						_ => Err(AssetError::Unsupported),
					}
				}
			}

			impl From<Asset> for ForeignChain {
				fn from(_asset: Asset) -> Self {
					ForeignChain::$chain
				}
			}

			#[test]
			fn consistency_check() {
				$(
					assert_eq!(
						ForeignChain::from(any::Asset::from(Asset::$asset)),
						ForeignChain::from(Asset::$asset),
						"Inconsistent asset type definition. Asset {} defined in {}, but mapped to chain {:?}",
						stringify!($asset),
						stringify!($mod),
						ForeignChain::from(any::Asset::from(Asset::$asset)),
					);
				)+
			}
		}
	};
}

// Defines each chain's Asset enum.
// Must be consistent with the mapping defined in any::Asset
chain_assets!(eth, Ethereum, Eth, Flip, Usdc);
chain_assets!(dot, Polkadot, Dot);
chain_assets!(btc, Bitcoin, Btc);

#[cfg(test)]
mod test_assets {
	use super::*;

	macro_rules! assert_conversion {
		($mod:ident, $asset:ident) => {
			assert_eq!(any::Asset::from($mod::Asset::$asset), any::Asset::$asset);
			assert_eq!(any::Asset::$asset, $mod::Asset::$asset.try_into().unwrap());
		};
	}

	macro_rules! assert_incompatible {
		($mod:ident, $asset:ident) => {
			assert!($mod::Asset::try_from(any::Asset::$asset).is_err());
		};
	}

	#[test]
	fn asset_id_to_asset() {
		assert!(Asset::try_from(0).is_err());
		assert_eq!(Asset::try_from(1).unwrap(), Asset::Eth);
		assert_eq!(Asset::try_from(2).unwrap(), Asset::Flip);
		assert_eq!(Asset::try_from(3).unwrap(), Asset::Usdc);
		assert_eq!(Asset::try_from(4).unwrap(), Asset::Dot);
		assert_eq!(Asset::try_from(5).unwrap(), Asset::Btc);
	}

	#[test]
	fn test_conversion() {
		assert_conversion!(eth, Eth);
		assert_conversion!(eth, Flip);
		assert_conversion!(eth, Usdc);
		assert_conversion!(dot, Dot);
		assert_conversion!(btc, Btc);

		assert_incompatible!(eth, Dot);
		assert_incompatible!(dot, Eth);
		assert_incompatible!(dot, Flip);
		assert_incompatible!(dot, Usdc);
		assert_incompatible!(btc, Usdc);
	}
}
