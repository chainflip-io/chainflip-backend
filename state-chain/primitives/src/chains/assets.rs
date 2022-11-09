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
//! assert_eq!(any::Asset::Flip, any::Asset::from(eth::Asset::Flip));
//! ```
use super::*;

#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};

pub mod any {
	use super::*;
	pub type Chain = AnyChain;

	/// A token or currency that can be swapped natively in the Chainflip AMM.
	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy, Hash)]
	#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
	pub enum Asset {
		Eth,
		Flip,
		Usdc,
		Dot,
	}

	impl From<Asset> for ForeignChain {
		fn from(asset: Asset) -> Self {
			match asset {
				Asset::Eth => Self::Ethereum,
				Asset::Flip => Self::Ethereum,
				Asset::Usdc => Self::Ethereum,
				Asset::Dot => Self::Polkadot,
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

            #[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Copy, Hash)]
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
        }
	};
}

chain_assets!(eth, Ethereum, Eth, Flip, Usdc);
chain_assets!(dot, Polkadot, Dot);

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
	fn test_conversion() {
		assert_conversion!(eth, Eth);
		assert_conversion!(eth, Flip);
		assert_conversion!(eth, Usdc);
		assert_conversion!(dot, Dot);

		assert_incompatible!(eth, Dot);
		assert_incompatible!(dot, Eth);
		assert_incompatible!(dot, Flip);
		assert_incompatible!(dot, Usdc);
	}
}
