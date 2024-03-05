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

macro_rules! assets {
	(pub enum Asset {
		$(($chain_mod:ident, $chain:ident, $chain_str:literal) => {
			($gas_asset:ident, $gas_lowercase:ident) = $gas_value:literal (GAS_ASSET)
			$(,($asset:ident, $lowercase:ident) = $value:literal)* $(,)?
		}),*$(,)?
	}) => {
		pub mod any {
			use strum_macros::EnumIter;
			use codec::{MaxEncodedLen, Encode, Decode};
			use scale_info::TypeInfo;
			use serde::{Serialize, Deserialize};
			use core::ops::IndexMut;
			use core::ops::Index;

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
				EnumIter,
			)]
			#[repr(u32)]
			pub enum Asset {
				$(
					$gas_asset = $gas_value,
					$($asset = $value,)*
				)*
			}
			impl TryFrom<u32> for Asset {
				type Error = &'static str;

				fn try_from(n: u32) -> Result<Self, Self::Error> {
					match n {
						$(
							x if x == Self::$gas_asset as u32 => Ok(Self::$gas_asset),
							$(x if x == Self::$asset as u32 => Ok(Self::$asset),)*
						)*
						_ => Err("Invalid asset id"),
					}
				}
			}
			impl Asset {
				pub fn all() -> impl Iterator<Item = Self> + 'static {
					use strum::IntoEnumIterator;
					Self::iter()
				}
			}
			impl From<Asset> for $crate::ForeignChain {
				fn from(asset: Asset) -> Self {
					match asset {
						$(
							Asset::$gas_asset $(| Asset::$asset)* => Self::$chain,
						)*
					}
				}
			}
			impl core::fmt::Display for Asset {
				fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
					write!(f, "{}", match self {
						$(
							Asset::$gas_asset => stringify!($gas_asset),
							$(
								Asset::$asset => stringify!($asset),
							)*
						)*
					}.to_uppercase())
				}
			}
			impl core::str::FromStr for Asset {
				type Err = &'static str;

				fn from_str(s: &str) -> Result<Self, Self::Err> {
					match s.to_lowercase().as_str() {
						$(
							stringify!($gas_lowercase) => Ok(Asset::$gas_asset),
							$(stringify!($lowercase) => Ok(Asset::$asset),)*
						)*
						_ => Err("Unrecognized asset"),
					}
				}
			}
			pub use asset_serde_impls::SerdeAsset as OldAsset;
			pub(super) mod asset_serde_impls {
				use serde::{Serialize, Deserialize};

				/// DO NOT USE THIS TYPE. This is only public to allow consistency in behaviour for out of date RPCs and Runtime API functions, once we remove those apis (and replace them in PRO-1202)
				#[derive(Copy, Clone, Serialize, Deserialize)]
				#[derive(Debug, PartialEq, Eq, Hash, codec::Encode, codec::Decode, scale_info::TypeInfo, codec::MaxEncodedLen)] /* Remove these derives once PRO-1202 is done */
				#[repr(u32)]
				#[serde(rename_all = "UPPERCASE")]
				pub enum SerdeAsset {
					$(
						$gas_asset = $gas_value,
						$($asset = $value,)*
					)*
				}
				impl From<SerdeAsset> for super::Asset {
					fn from(serde_asset: SerdeAsset) -> Self {
						match serde_asset {
							$(
								SerdeAsset::$gas_asset => super::Asset::$gas_asset,
								$(SerdeAsset::$asset => super::Asset::$asset,)*
							)*
						}
					}
				}
				impl From<super::Asset> for SerdeAsset {
					fn from(asset: super::Asset) -> Self {
						match asset {
							$(
								super::Asset::$gas_asset => SerdeAsset::$gas_asset,
								$(super::Asset::$asset => SerdeAsset::$asset,)*
							)*
						}
					}
				}

				#[derive(Serialize, Deserialize)]
				#[serde(untagged)]
				#[serde(
					expecting = r#"Expected a valid asset specifier. Assets should be specified as upper-case strings, e.g. `"ETH"`, and can be optionally distinguished by chain, e.g. `{ chain: "Ethereum", asset: "ETH" }."#
				)]
				enum SerdeAssetOptionalExplicitChain {
					ImplicitChain(SerdeAsset),
					ExplicitChain { chain: Option<$crate::ForeignChain>, asset: SerdeAsset },
				}

				impl Serialize for super::Asset {
					fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
						where S: serde::Serializer
					{
						Serialize::serialize(&SerdeAssetOptionalExplicitChain::ExplicitChain {
							chain: Some((*self).into()), asset: (*self).into()
						}, serializer)
					}
				}
				impl<'de> Deserialize<'de> for super::Asset {
					fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
					   where D: serde::Deserializer<'de> {
						<SerdeAssetOptionalExplicitChain as Deserialize<'de>>::deserialize(deserializer).and_then(|serde_asset_optional_explicit_chain| {
							let serde_asset = match serde_asset_optional_explicit_chain {
								SerdeAssetOptionalExplicitChain::ImplicitChain(serde_asset) | SerdeAssetOptionalExplicitChain::ExplicitChain { chain: None, asset: serde_asset } => serde_asset,
								SerdeAssetOptionalExplicitChain::ExplicitChain {
									chain: Some(serde_chain),
									asset: serde_asset
								} => {
									let asset_chain = match serde_asset {
										$(
											SerdeAsset::$gas_asset $(| SerdeAsset::$asset)* => $crate::ForeignChain::$chain,
										)*
									};

									if asset_chain != serde_chain {
										return Err(<<D as serde::Deserializer<'de>>::Error as serde::de::Error>::custom(lazy_format::lazy_format!("The asset '{asset}' does not exist on the '{serde_chain}' chain, but is instead a '{asset_chain}' asset. Either try using '{{\"chain\":\"{asset_chain}\", \"asset\":\"{asset}\"}}', or use a different asset (i.e. '{example_chain_asset}') ", asset = super::Asset::from(serde_asset), example_chain_asset = match serde_chain {
											$(
												$crate::ForeignChain::$chain => super::Asset::$gas_asset,
											)*
										})))
									} else {
										serde_asset
									}
								},
							};

							Ok(serde_asset.into())
						})
					}
				}

				#[cfg(test)]
				mod tests {
					use serde_json;
					use cf_utilities::assert_ok;
					use super::super::Asset;

					#[test]
					fn test_asset_serde_encoding() {
						assert_eq!(assert_ok!(serde_json::to_string(&Asset::Eth)), "{\"chain\":\"Ethereum\",\"asset\":\"ETH\"}");
						assert_eq!(assert_ok!(serde_json::to_string(&Asset::Dot)), "{\"chain\":\"Polkadot\",\"asset\":\"DOT\"}");
						assert_eq!(assert_ok!(serde_json::to_string(&Asset::Btc)), "{\"chain\":\"Bitcoin\",\"asset\":\"BTC\"}");

						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("{\"chain\":\"Ethereum\",\"asset\":\"ETH\"}")), Asset::Eth);
						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("{\"chain\":\"Polkadot\",\"asset\":\"DOT\"}")), Asset::Dot);
						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("{\"chain\":\"Bitcoin\",\"asset\":\"BTC\"}")), Asset::Btc);

						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("{\"asset\":\"ETH\"}")), Asset::Eth);
						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("{\"asset\":\"DOT\"}")), Asset::Dot);
						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("{\"asset\":\"BTC\"}")), Asset::Btc);

						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("\"ETH\"")), Asset::Eth);
						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("\"DOT\"")), Asset::Dot);
						assert_eq!(assert_ok!(serde_json::from_str::<Asset>("\"BTC\"")), Asset::Btc);
					}
				}
			}

			#[derive(
				Copy,
				Clone,
				Debug,
				PartialEq,
				Eq,
				Hash,
			)]
			pub enum ForeignChainAndAsset {
				$(
					$chain(super::$chain_mod::Asset),
				)*
			}
			impl From<Asset> for ForeignChainAndAsset {
				fn from(value: Asset) -> Self {
					match value {
						$(
							Asset::$gas_asset => Self::$chain(super::$chain_mod::Asset::$gas_asset),
							$(Asset::$asset => Self::$chain(super::$chain_mod::Asset::$asset),)*
						)*
					}
				}
			}
			impl From<ForeignChainAndAsset> for Asset {
				fn from(value: ForeignChainAndAsset) -> Self {
					match value {
						$(
							ForeignChainAndAsset::$chain(super::$chain_mod::Asset::$gas_asset) => Self::$gas_asset,
							$(ForeignChainAndAsset::$chain(super::$chain_mod::Asset::$asset) => Self::$asset,)*
						)*
					}
				}
			}

			#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
			pub struct AssetMap<T> {
				$(
					#[serde(rename = $chain_str)]
					pub $chain_mod: super::$chain_mod::AssetMap::<T>,
				)*
			}
			impl<T> AssetMap<T> {
				pub fn from_fn<F: FnMut(Asset) -> T>(mut f: F) -> Self {
					Self {
						$($chain_mod: super::$chain_mod::AssetMap::<T>::from_fn(|asset| f(asset.into())),)*
					}
				}

				pub fn try_from_fn<E, F: FnMut(Asset) -> Result<T, E>>(mut f: F) -> Result<Self, E> {
					Ok(Self {
						$($chain_mod: super::$chain_mod::AssetMap::<T>::try_from_fn(|asset| f(asset.into()))?,)*
					})
				}

				pub fn map<R, F: FnMut(T) -> R>(self, mut f: F) -> AssetMap<R> {
					AssetMap {
						$($chain_mod: self.$chain_mod.map(&mut f),)*
					}
				}

				/// TODO: Remove this function, once PRO-1202 is complete
				pub fn try_from_iter<Iter: Iterator<Item = (Asset, T)> + Clone>(iter: Iter) -> Option<Self> {
					Self::try_from_fn(|required_asset| {
						iter.clone().find(|(asset, _t)| *asset == required_asset).ok_or(()).map(|x| x.1)
					}).ok()
				}
			}

			impl<T> IndexMut<Asset> for AssetMap<T> {
				fn index_mut(&mut self, index: Asset) -> &mut T {
					match index {
						$(
							Asset::$gas_asset => &mut self.$chain_mod.$gas_lowercase,
							$(Asset::$asset => &mut self.$chain_mod.$lowercase,)*
						)*
					}
				}
			}

			impl<T> Index<Asset> for AssetMap<T> {
				type Output = T;
				fn index(&self, index: Asset) -> &T {
					match index {
						$(
							Asset::$gas_asset => &self.$chain_mod.$gas_lowercase,
							$(Asset::$asset => &self.$chain_mod.$lowercase,)*
						)*
					}
				}
			}
		}

		$(
			pub mod $chain_mod {
				use super::any;
				use codec::{MaxEncodedLen, Encode, Decode};
				use scale_info::TypeInfo;
				use serde::{Serialize, Deserialize};

				pub type Chain = $crate::chains::$chain;
				pub const GAS_ASSET: Asset = Asset::$gas_asset;

				#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Hash, Serialize, Deserialize)]
				pub enum Asset {
					$gas_asset,
					$($asset,)*
				}
				impl From<Asset> for any::Asset {
					fn from(asset: Asset) -> Self {
						match asset {
							Asset::$gas_asset => any::Asset::$gas_asset,
							$(
								Asset::$asset => any::Asset::$asset,
							)*
						}
					}
				}
				impl From<Asset> for $crate::ForeignChain {
					fn from(_asset: Asset) -> Self {
						Self::$chain
					}
				}
				impl TryFrom<super::any::Asset> for Asset {
					type Error = AssetError;

					fn try_from(asset: super::any::Asset) -> Result<Self, Self::Error> {
						match asset {
							super::any::Asset::$gas_asset => Ok(Asset::$gas_asset),
							$(
								super::any::Asset::$asset => Ok(Asset::$asset),
							)*
							_ => Err(AssetError::Unsupported),
						}
					}
				}

				#[derive(Clone, Debug, TypeInfo, Encode, Decode, MaxEncodedLen)]
				pub enum AssetError {
					Unsupported,
				}

				#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
				#[serde(rename_all = "UPPERCASE")]
				pub struct AssetMap<T> {
					pub $gas_lowercase: T,
					$(
						pub $lowercase: T,
					)*
				}
				impl<T> AssetMap<T> {
					pub fn from_fn<F: FnMut(Asset) -> T>(mut f: F) -> Self {
						Self {
							$gas_lowercase: f(Asset::$gas_asset),
							$($lowercase: f(Asset::$asset),)*
						}
					}

					pub fn try_from_fn<E, F: FnMut(Asset) -> Result<T, E>>(mut f: F) -> Result<Self, E> {
						Ok(Self {
							$gas_lowercase: f(Asset::$gas_asset)?,
							$($lowercase: f(Asset::$asset)?,)*
						})
					}

					pub fn map<R, F: FnMut(T) -> R>(self, mut f: F) -> AssetMap<R> {
						AssetMap {
							$gas_lowercase: f(self.$gas_lowercase),
							$($lowercase: f(self.$lowercase),)*
						}
					}
				}
			}

		)*
	}
}

// !!!!!! IMPORTANT !!!!!!
// Do not change these indices, or the orderings (as the orderings will effect some serde formats
// (But not JSON), and the scale encoding)
assets!(pub enum Asset {
	// 0 is reserved for particular cross chain messaging scenarios where we want to pass
	// through a message without making a swap.
	(eth, Ethereum, "Ethereum") => {
		(Eth, eth) = 1u32 (GAS_ASSET),
		(Flip, flip) = 2u32,
		(Usdc, usdc) = 3u32,
		(Usdt, usdt) = 6u32,
	},
	(dot, Polkadot, "Polkadot") => {
		(Dot, dot) = 4u32 (GAS_ASSET),
	},
	(btc, Bitcoin, "Bitcoin") => {
		(Btc, btc) = 5u32 (GAS_ASSET),
	},
});

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
		assert!(any::Asset::try_from(0).is_err());
		assert_eq!(any::Asset::try_from(1).unwrap(), any::Asset::Eth);
		assert_eq!(any::Asset::try_from(2).unwrap(), any::Asset::Flip);
		assert_eq!(any::Asset::try_from(3).unwrap(), any::Asset::Usdc);
		assert_eq!(any::Asset::try_from(4).unwrap(), any::Asset::Dot);
		assert_eq!(any::Asset::try_from(5).unwrap(), any::Asset::Btc);
		assert_eq!(any::Asset::try_from(6).unwrap(), any::Asset::Usdt);
	}

	#[test]
	fn test_conversion() {
		assert_conversion!(eth, Eth);
		assert_conversion!(eth, Flip);
		assert_conversion!(eth, Usdc);
		assert_conversion!(eth, Usdt);
		assert_conversion!(dot, Dot);
		assert_conversion!(btc, Btc);

		assert_incompatible!(eth, Dot);
		assert_incompatible!(dot, Eth);
		assert_incompatible!(dot, Flip);
		assert_incompatible!(dot, Usdc);
		assert_incompatible!(btc, Usdc);
		assert_incompatible!(btc, Usdt);
	}
}
