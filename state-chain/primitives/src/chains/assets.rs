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
	(@ legacy_encoding) => {};
	(
		$(
			Chain {
				variant: $chain_variant:ident,
				member_and_module: $chain_member_and_module:ident,
				string: $chain_string:literal $((aliases: [$($chain_string_aliases:literal),+$(,)?]))?,
				json: $chain_json:literal,
				assets: [
					$(
						Asset {
							variant: $asset_variant:ident,
							member: $asset_member:ident,
							string: $asset_string:literal $((aliases: [$($asset_string_aliases:literal),+$(,)?]))?,
							json: $asset_json:literal,
							gas: $asset_gas:literal,
							index: $asset_index:literal
							$(,$asset_legacy_encoding:tt)?$(,)?
						}
					),+$(,)?
				]$(,)?
			}
		),+$(,)?
	) => {
		// This forces $asset_legacy_encoding to only ever possibly be `legacy_encoding`. This allows
		// the option legacy_enoding to be an optional, but also be fixed and referenceable without
		// needing a full incremental tt muncher.
		$(
			$(
				$(assets!(@ $asset_legacy_encoding);)?
			)+
		)+

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
					$($asset_variant = $asset_index,)+
				)+
			}
			impl TryFrom<u32> for Asset {
				type Error = &'static str;

				fn try_from(n: u32) -> Result<Self, Self::Error> {
					match n {
						$(
							$(x if x == Self::$asset_variant as u32 => Ok(Self::$asset_variant),)+
						)+
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
							$(Asset::$asset_variant)|+ => Self::$chain_variant,
						)+
					}
				}
			}
			impl core::fmt::Display for Asset {
				fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
					write!(f, "{}", match self {
						$(
							$(
								Asset::$asset_variant => $asset_string,
							)+
						)+
					})
				}
			}
			impl core::str::FromStr for Asset {
				type Err = &'static str;

				fn from_str(s: &str) -> Result<Self, Self::Err> {
					if let Some((prefix, suffix)) = s.split_once('-') {
						match prefix {
							$(
								$chain_string => {
									match suffix {
										$(
											$asset_string $($(|$asset_string_aliases)+)? => Ok(Self::$asset_variant),
										)+
										_ => Err(concat!("Unrecognized ", $chain_string, " asset"))
									}
								},
							)+
							_ => Err("Unrecognized chain")
						}
					} else {
						Err("Unrecognized asset, expected the format \"<chain>:<asset>\"")
					}
				}
			}

			/// DO NOT USE THIS TYPE. This exists to allow consistency in behaviour for out of date RPCs and Runtime API functions,
			/// once we remove those apis (and replace them in PRO-1202) this can be removed.
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
			)]
			#[repr(u32)]
			pub enum OldAsset {
				$(
					$($asset_variant = $asset_index,)+
				)+
			}
			impl From<OldAsset> for Asset {
				fn from(value: OldAsset) -> Self {
					match value {
						$(
							$(OldAsset::$asset_variant => Asset::$asset_variant,)+
						)+
					}
				}
			}
			impl From<Asset> for OldAsset {
				fn from(value: Asset) -> Self {
					match value {
						$(
							$(Asset::$asset_variant => OldAsset::$asset_variant,)+
						)+
					}
				}
			}

			pub(super) mod serde_impls {
				use serde::{Serialize, Deserialize};

				#[derive(Serialize, Deserialize)]
				#[serde(untagged)]
				#[serde(
					expecting = r#"Expected a valid asset specifier. Assets should be specified as upper-case strings, e.g. `"ETH"`, and can be optionally distinguished by chain, e.g. `{ chain: "Ethereum", asset: "ETH" }."#
				)]
				enum SerdeAssetOptionalExplicitChain {
					Implicit(serde_utils::SerdeImplicitChainAsset),
					Explicit(serde_utils::SerdeExplicitChainAsset),
					StructuredImplicit { asset: serde_utils::SerdeImplicitChainAsset }
				}

				impl Serialize for super::Asset {
					fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
						where S: serde::Serializer
					{
						Serialize::serialize(&SerdeAssetOptionalExplicitChain::Explicit((*self).into()), serializer)
					}
				}
				impl<'de> Deserialize<'de> for super::Asset {
					fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
					   where D: serde::Deserializer<'de> {
						<SerdeAssetOptionalExplicitChain as Deserialize<'de>>::deserialize(deserializer).map(|serde_asset_optional_explicit_chain| {
							match serde_asset_optional_explicit_chain {
								SerdeAssetOptionalExplicitChain::Implicit(implicit_chain_asset) | SerdeAssetOptionalExplicitChain::StructuredImplicit { asset: implicit_chain_asset } => implicit_chain_asset.into(),
								SerdeAssetOptionalExplicitChain::Explicit(explicit_chain_asset) => explicit_chain_asset.into(),
							}
						})
					}
				}

				impl Serialize for super::OldAsset {
					fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
						where S: serde::Serializer
					{
						Serialize::serialize(
							&if let Ok(implicit_chain_asset) = super::Asset::from(*self).try_into() {
								SerdeAssetOptionalExplicitChain::Implicit(implicit_chain_asset)
							} else {
								SerdeAssetOptionalExplicitChain::Explicit(super::Asset::from(*self).into())
							},
							serializer
						)
					}
				}
				impl<'de> Deserialize<'de> for super::OldAsset {
					fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
					   where D: serde::Deserializer<'de> {
						super::Asset::deserialize(deserializer).map(Into::into)
					}
				}

				mod serde_utils {
					use serde::{Serialize, Deserialize};
					use super::super::super::any;

					$(
						mod $chain_member_and_module {
							use serde::{Serialize, Deserialize};

							#[derive(Serialize, Deserialize)]
							pub enum SerdeChain {
								#[serde(rename = $chain_json)]
								$chain_variant
							}
						}
					)+

					#[derive(Serialize, Deserialize)]
					#[serde(untagged)]
					pub(super) enum SerdeExplicitChainAsset {
						$(
							$chain_variant{ chain: $chain_member_and_module::SerdeChain, asset: super::super::super::$chain_member_and_module::Asset }
						),+
					}
					impl From<any::Asset> for SerdeExplicitChainAsset {
						fn from(value: any::Asset) -> Self {
							match value {
								$(
									$(
										any::Asset::$asset_variant => Self::$chain_variant { chain: $chain_member_and_module::SerdeChain::$chain_variant, asset: super::super::super::$chain_member_and_module::Asset::$asset_variant },
									)+
								)+
							}
						}
					}
					impl From<SerdeExplicitChainAsset> for any::Asset {
						fn from(value: SerdeExplicitChainAsset) -> any::Asset {
							match value {
								$(
									$(
										SerdeExplicitChainAsset::$chain_variant { chain: _, asset: super::super::super::$chain_member_and_module::Asset::$asset_variant } => Self::$asset_variant,
									)+
								)+
							}
						}
					}

					#[derive(Serialize, Deserialize)]
					#[repr(u32)]
					pub(super) enum SerdeImplicitChainAsset {
						$(
							$(
								$(
									#[serde(rename = $asset_json)]
									// IMPORTANT: This doc attribute is needed so we can only include variants/assets that have the `legacy_encoding`` option.
									#[doc = stringify!($asset_legacy_encoding)]
									$asset_variant = $asset_index,
								)?
							)+
						)+
					}
					impl TryFrom<any::Asset> for SerdeImplicitChainAsset {
						type Error = ();

						#[allow(unreachable_patterns)]
						#[allow(unused_variables)]
						fn try_from(value: any::Asset) -> Result<Self, Self::Error> {
							match value {
								$(
									$(
										$(
											any::Asset::$asset_variant => { let $asset_legacy_encoding = (); Ok(Self::$asset_variant) },
										)?
									)+
								)+
								_ => Err(())
							}
						}
					}
					impl From<SerdeImplicitChainAsset> for any::Asset {
						#[allow(unused_variables)]
						fn from(value: SerdeImplicitChainAsset) -> Self {
							match value {
								$(
									$(
										$(
											SerdeImplicitChainAsset::$asset_variant => { let $asset_legacy_encoding = (); Self::$asset_variant },
										)?
									)+
								)+
							}
						}
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
					$chain_variant(super::$chain_member_and_module::Asset),
				)+
			}
			impl From<Asset> for ForeignChainAndAsset {
				fn from(value: Asset) -> Self {
					match value {
						$(
							$(Asset::$asset_variant => Self::$chain_variant(super::$chain_member_and_module::Asset::$asset_variant),)+
						)+
					}
				}
			}
			impl From<ForeignChainAndAsset> for Asset {
				fn from(value: ForeignChainAndAsset) -> Self {
					match value {
						$(
							$(ForeignChainAndAsset::$chain_variant(super::$chain_member_and_module::Asset::$asset_variant) => Self::$asset_variant,)+
						)+
					}
				}
			}

			#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Encode, Decode, TypeInfo, MaxEncodedLen, Default)]
			pub struct AssetMap<T> {
				$(
					#[serde(rename = $chain_json)]
					pub $chain_member_and_module: super::$chain_member_and_module::AssetMap::<T>,
				)*
			}
			impl<T> AssetMap<T> {
				pub fn from_fn<F: FnMut(Asset) -> T>(mut f: F) -> Self {
					Self {
						$($chain_member_and_module: super::$chain_member_and_module::AssetMap::<T>::from_fn(|asset| f(asset.into())),)+
					}
				}

				pub fn try_from_fn<E, F: FnMut(Asset) -> Result<T, E>>(mut f: F) -> Result<Self, E> {
					Ok(Self {
						$($chain_member_and_module: super::$chain_member_and_module::AssetMap::<T>::try_from_fn(|asset| f(asset.into()))?,)+
					})
				}

				pub fn map<R, F: FnMut(T) -> R>(self, mut f: F) -> AssetMap<R> {
					AssetMap {
						$($chain_member_and_module: self.$chain_member_and_module.map(&mut f),)+
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
							$(Asset::$asset_variant => &mut self.$chain_member_and_module.$asset_member,)+
						)+
					}
				}
			}

			impl<T> Index<Asset> for AssetMap<T> {
				type Output = T;
				fn index(&self, index: Asset) -> &T {
					match index {
						$(
							$(Asset::$asset_variant => &self.$chain_member_and_module.$asset_member,)+
						)+
					}
				}
			}
		}

		$(
			pub mod $chain_member_and_module {
				use super::any;
				use codec::{MaxEncodedLen, Encode, Decode};
				use scale_info::TypeInfo;
				use serde::{Serialize, Deserialize};

				pub type Chain = $crate::chains::$chain_variant;
				pub const GAS_ASSET: Asset = {
					let mut gas_asset = None;

					$(
						if $asset_gas {
							assert!(gas_asset.is_none(), "Each chain can only have one gas asset.");
							gas_asset = Some(Asset::$asset_variant);
						}
					)+

					match gas_asset {
						Some(gas_asset) => gas_asset,
						None => panic!("Each chain must have exactly one gas asset.")
					}
				};

				#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, Hash, Serialize, Deserialize)]
				pub enum Asset {
					$(
						#[serde(rename = $asset_json)]
						$asset_variant,
					)+
				}
				impl From<Asset> for any::Asset {
					fn from(asset: Asset) -> Self {
						match asset {
							$(
								Asset::$asset_variant => any::Asset::$asset_variant,
							)+
						}
					}
				}
				impl From<Asset> for $crate::ForeignChain {
					fn from(_asset: Asset) -> Self {
						Self::$chain_variant
					}
				}
				impl TryFrom<super::any::Asset> for Asset {
					type Error = AssetError;

					fn try_from(asset: super::any::Asset) -> Result<Self, Self::Error> {
						match asset {
							$(
								super::any::Asset::$asset_variant => Ok(Asset::$asset_variant),
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
				pub struct AssetMap<T> {
					$(
						#[serde(rename = $asset_json)]
						pub $asset_member: T,
					)+
				}
				impl<T> AssetMap<T> {
					pub fn from_fn<F: FnMut(Asset) -> T>(mut f: F) -> Self {
						Self {
							$($asset_member: f(Asset::$asset_variant),)+
						}
					}

					pub fn try_from_fn<E, F: FnMut(Asset) -> Result<T, E>>(mut f: F) -> Result<Self, E> {
						Ok(Self {
							$($asset_member: f(Asset::$asset_variant)?,)+
						})
					}

					pub fn map<R, F: FnMut(T) -> R>(self, mut f: F) -> AssetMap<R> {
						AssetMap {
							$($asset_member: f(self.$asset_member),)+
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
assets!(
	// 0 is reserved for particular cross chain messaging scenarios where we want to pass
	// through a message without making a swap.
	Chain {
		variant: Ethereum,
		member_and_module: eth,
		string: "Ethereum" (aliases: ["ETHEREUM", "ethereum"]),
		json: "Ethereum",
		assets: [
			Asset {
				variant: Eth,
				member: eth,
				string: "ETH" (aliases: ["Eth", "eth"]),
				json: "ETH",
				gas: true,
				index: 1,
				legacy_encoding,
			},
			Asset {
				variant: Flip,
				member: flip,
				string: "FLIP" (aliases: ["Flip", "flip"]),
				json: "FLIP",
				gas: false,
				index: 2,
				legacy_encoding,
			},
			Asset {
				variant: Usdc,
				member: usdc,
				string: "USDC" (aliases: ["Usdc", "usdc"]),
				json: "USDC",
				gas: false,
				index: 3,
				legacy_encoding,
			},
		],
	},
	Chain {
		variant: Polkadot,
		member_and_module: dot,
		string: "Polkadot" (aliases: ["POLKADOT", "polkadot"]),
		json: "Polkadot",
		assets: [
			Asset {
				variant: Dot,
				member: dot,
				string: "DOT" (aliases: ["Dot", "dot"]),
				json: "DOT",
				gas: true,
				index: 4,
				legacy_encoding,
			},
		],
	},
	Chain {
		variant: Bitcoin,
		member_and_module: btc,
		string: "Bitcoin" (aliases: ["BITCOIN", "bitcoin"]),
		json: "Bitcoin",
		assets: [
			Asset {
				variant: Btc,
				member: btc,
				string: "BTC" (aliases: ["Btc", "btc"]),
				json: "BTC",
				gas: true,
				index: 5,
				legacy_encoding,
			},
		],
	},
	Chain {
		variant: Arbitrum,
		member_and_module: arb,
		string: "Arbitrum" (aliases: ["ARBITRUM", "arbitrum"]),
		json: "Arbitrum",
		assets: [
			Asset {
				variant: ArbEth,
				member: eth,
				string: "ETH" (aliases: ["Eth", "eth"]),
				json: "ETH",
				gas: true,
				index: 6,
			},
			Asset {
				variant: ArbUsdc,
				member: usdc,
				string: "USDC" (aliases: ["Usdc", "usdc"]),
				json: "USDC",
				gas: false,
				index: 7,
			},
		],
	}
);

#[cfg(test)]
mod test_assets {
	use super::*;
	use cf_utilities::{assert_ok, assert_err};

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
		assert_eq!(any::Asset::try_from(6).unwrap(), any::Asset::ArbEth);
		assert_eq!(any::Asset::try_from(7).unwrap(), any::Asset::ArbUsdc);
	}

	#[test]
	fn test_conversion() {
		assert_conversion!(eth, Eth);
		assert_conversion!(eth, Flip);
		assert_conversion!(eth, Usdc);
		assert_conversion!(dot, Dot);
		assert_conversion!(btc, Btc);
		assert_conversion!(arb, ArbEth);
		assert_conversion!(arb, ArbUsdc);

		assert_incompatible!(eth, Dot);
		assert_incompatible!(dot, Eth);
		assert_incompatible!(dot, Flip);
		assert_incompatible!(dot, Usdc);
		assert_incompatible!(btc, Usdc);
	}

	#[test]
	fn test_asset_encoding() {
		use core::{fmt::Debug, str::FromStr};
		use serde::de::DeserializeOwned;

		// FromStr

		assert_eq!(assert_ok!(any::Asset::from_str("Ethereum-ETH")), any::Asset::Eth);
		assert_eq!(assert_ok!(any::Asset::from_str("Polkadot-DOT")), any::Asset::Dot);
		assert_eq!(assert_ok!(any::Asset::from_str("Bitcoin-BTC")), any::Asset::Btc);
		assert_eq!(assert_ok!(any::Asset::from_str("Ethereum-eth")), any::Asset::Eth);
		assert_eq!(assert_ok!(any::Asset::from_str("Ethereum-Eth")), any::Asset::Eth);
		assert_eq!(assert_ok!(any::Asset::from_str("Arbitrum-Eth")), any::Asset::ArbEth);

		assert_err!(any::Asset::from_str("Ethereum-BTC"));
		assert_err!(any::Asset::from_str("Polkadot-USDC"));
		assert_err!(any::Asset::from_str("Arbitrum-Btc"));
		assert_err!(any::Asset::from_str("Terra-ETH"));

		// Serialization

		assert_eq!(
			assert_ok!(serde_json::to_string(&any::Asset::Eth)),
			"{\"chain\":\"Ethereum\",\"asset\":\"ETH\"}"
		);
		assert_eq!(
			assert_ok!(serde_json::to_string(&any::Asset::Dot)),
			"{\"chain\":\"Polkadot\",\"asset\":\"DOT\"}"
		);
		assert_eq!(
			assert_ok!(serde_json::to_string(&any::Asset::Btc)),
			"{\"chain\":\"Bitcoin\",\"asset\":\"BTC\"}"
		);
		assert_eq!(
			assert_ok!(serde_json::to_string(&any::Asset::ArbEth)),
			"{\"chain\":\"Arbitrum\",\"asset\":\"ETH\"}"
		);

		assert_eq!(assert_ok!(serde_json::to_string(&any::OldAsset::Eth)), "\"ETH\"");
		assert_eq!(assert_ok!(serde_json::to_string(&any::OldAsset::Dot)), "\"DOT\"");
		assert_eq!(assert_ok!(serde_json::to_string(&any::OldAsset::Btc)), "\"BTC\"");
		assert_eq!(
			assert_ok!(serde_json::to_string(&any::OldAsset::ArbEth)),
			"{\"chain\":\"Arbitrum\",\"asset\":\"ETH\"}"
		);

		// Explicit Chain Deserialization

		fn explicit_chain_deserialization<T: DeserializeOwned + Debug + From<any::Asset> + Eq>() {
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"chain\":\"Ethereum\",\"asset\":\"ETH\"}")),
				T::from(any::Asset::Eth)
			);
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"chain\":\"Polkadot\",\"asset\":\"DOT\"}")),
				T::from(any::Asset::Dot)
			);
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"chain\":\"Bitcoin\",\"asset\":\"BTC\"}")),
				T::from(any::Asset::Btc)
			);
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"chain\":\"Arbitrum\",\"asset\":\"ETH\"}")),
				T::from(any::Asset::ArbEth)
			);

			assert_err!(serde_json::from_str::<T>("{\"chain\":\"Ethereum\",\"asset\":\"Eth\"}"));
			assert_err!(serde_json::from_str::<T>("{\"chain\":\"Polkadot\",\"asset\":\"Dot\"}"));
			assert_err!(serde_json::from_str::<T>("{\"chain\":\"Bitcoin\",\"asset\":\"Btc\"}"));
			assert_err!(serde_json::from_str::<T>("{\"chain\":\"ETHEREUM\",\"asset\":\"ETH\"}"));
			assert_err!(serde_json::from_str::<T>("{\"chain\":\"ETHEREUM\",\"asset\":\"BTC\"}"));
			assert_err!(serde_json::from_str::<T>("{\"chain\":\"ETHEREUM\",\"asset\":\"eth\"}"));
		}

		explicit_chain_deserialization::<any::Asset>();
		explicit_chain_deserialization::<any::OldAsset>();

		// Implicit Chain Deserialization

		fn implicit_chain_deserialization<
			T: DeserializeOwned + Debug + From<any::Asset> + Eq,
		>() {
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"asset\":\"ETH\"}")),
				T::from(any::Asset::Eth)
			);
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"asset\":\"DOT\"}")),
				T::from(any::Asset::Dot)
			);
			assert_eq!(
				assert_ok!(serde_json::from_str::<T>("{\"asset\":\"BTC\"}")),
				T::from(any::Asset::Btc)
			);

			assert_err!(serde_json::from_str::<T>("{\"asset\":\"MEH\"}"));
			assert_err!(serde_json::from_str::<T>("{\"asset\":\"eTH\"}"));
			assert_err!(serde_json::from_str::<T>("{\"asset\":\"DOt\"}"));
			assert_err!(serde_json::from_str::<T>("{\"asset\":\"BtC\"}"));
		}

		implicit_chain_deserialization::<any::Asset>();
		implicit_chain_deserialization::<any::OldAsset>();

		// Unstructured Implicit Chain Deserialization

		fn unstructured_implicit_chain_deserialization<T: DeserializeOwned + Debug + From<any::Asset> + Eq>() {
			assert_eq!(assert_ok!(serde_json::from_str::<T>("\"ETH\"")), T::from(any::Asset::Eth));
			assert_eq!(assert_ok!(serde_json::from_str::<T>("\"DOT\"")), T::from(any::Asset::Dot));
			assert_eq!(assert_ok!(serde_json::from_str::<T>("\"BTC\"")), T::from(any::Asset::Btc));

			assert_err!(serde_json::from_str::<T>("\"eTh\""));
			assert_err!(serde_json::from_str::<T>("\"dOt\""));
			assert_err!(serde_json::from_str::<T>("\"bTc\""));
		}

		unstructured_implicit_chain_deserialization::<any::Asset>();
		unstructured_implicit_chain_deserialization::<any::OldAsset>();
	}
}
