// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use cf_amm::math::Price;
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};
use subxt::{
	config::{substrate::BlakeTwo256, DefaultTransactionExtensions},
	ArcMetadata, Config,
};

/// Caches metadata per spec version so that working at a block doesn't re-download it.
#[derive(Debug, Clone, Default)]
pub struct StateChainConfig {
	metadata_for_spec_version: Arc<RwLock<HashMap<u32, ArcMetadata>>>,
	/// Set for offline decoding, where every block is treated as using the given metadata.
	fixed_spec_and_transaction_version: Option<(u32, u32)>,
}

impl StateChainConfig {
	/// Config for an [subxt::OfflineClient] that decodes everything with the given metadata.
	pub fn with_fixed_metadata(metadata: ArcMetadata) -> Self {
		const SPEC_VERSION: u32 = 0;
		let config = Self {
			fixed_spec_and_transaction_version: Some((SPEC_VERSION, 0)),
			..Default::default()
		};
		config.set_metadata_for_spec_version(SPEC_VERSION, metadata);
		config
	}
}

impl Config for StateChainConfig {
	// We cannot use our own Runtime's types for every associated type here, see comments below.
	type AccountId = subxt::utils::AccountId32; // Requires EncodeAsType trait (which our AccountId doesn't)
	type Address = subxt::utils::MultiAddress<Self::AccountId, ()>; // Must be convertible from Self::AccountId
	type Signature = subxt::utils::MultiSignature; // Requires EncodeAsType/DecodeAsType
	type Hasher = BlakeTwo256;
	type Header =
		subxt::config::substrate::SubstrateHeader<<Self::Hasher as subxt::config::Hasher>::Hash>;
	type AssetId = u32; // Not used - we don't use pallet-assets
	type TransactionExtensions = DefaultTransactionExtensions<Self>;

	fn spec_and_transaction_version_for_block_number(
		&self,
		_block_number: u64,
	) -> Option<(u32, u32)> {
		self.fixed_spec_and_transaction_version
	}

	fn metadata_for_spec_version(&self, spec_version: u32) -> Option<ArcMetadata> {
		self.metadata_for_spec_version.read().ok()?.get(&spec_version).cloned()
	}

	fn set_metadata_for_spec_version(&self, spec_version: u32, metadata: ArcMetadata) {
		if let Ok(mut cache) = self.metadata_for_spec_version.write() {
			cache.insert(spec_version, metadata);
		}
	}
}

// The subxt macro (defined in the build script), generates a strongly typed API from a WASM file.
// All cf types are substituted with new corresponding types that implement some extra traits,
// allowing subxt to scale encode/decode these cf types. However, this makes it challenging to
// convert from the newly generated subxt types to cf types, especially for complex hierarchical
// types. The trick is use the `substitute_type` directive to instruct the subxt macro to use
// certain types in place of the default generated types. For example:
// ```ignore
// substitute_type(
// 	 path = "cf_chains::address::EncodedAddress",
//   with = "::subxt::utils::Static<cf_chains::address::EncodedAddress>"
// ),
// ```
// * This will generate: ::subxt::utils::Static<cf_chains::address::EncodedAddress> in place of the
//   default runtime_types::cf_chains::address::EncodedAddress
// * The `::subxt::utils::Static` is required to wrap the type and implement the necessary
//   `EncodeAsType` and `DecodeAsType` traits.
// * Any cf type that needs to be substituted must be defined in the `substitute_type` directive.
// * For any other complex types with multiple hierarchies or generics, please add manual conversion
//   functions below.
include!(concat!(env!("OUT_DIR"), "/cf_static_runtime.rs"));

// Conversions from cf_static_runtime::runtime_types
impl<T>
	From<
		cf_static_runtime::runtime_types::cf_chains::refund_parameters::ChannelRefundParameters<
			T,
			Option<
				cf_static_runtime::runtime_types::cf_chains::CcmChannelMetadata<
					cf_static_runtime::runtime_types::cf_chains::CcmAdditionalData,
				>,
			>,
		>,
	> for cf_chains::ChannelRefundParametersUnchecked<T>
{
	fn from(
		value: cf_static_runtime::runtime_types::cf_chains::refund_parameters::ChannelRefundParameters<
			T,
			Option<
				cf_static_runtime::runtime_types::cf_chains::CcmChannelMetadata<
					cf_static_runtime::runtime_types::cf_chains::CcmAdditionalData,
				>,
			>,
		>,
	) -> Self {
		Self {
			retry_duration: value.retry_duration,
			refund_address: value.refund_address,
			min_price: Price::from_raw(*value.min_price.0),
			refund_ccm_metadata: value.refund_ccm_metadata.map(|metadata| {
				cf_chains::CcmChannelMetadata {
					message: cf_chains::CcmMessage::try_from(metadata.message.0)
						.expect("Runtime message exceeds 15,000 bytes"),
					gas_budget: metadata.gas_budget,
					ccm_additional_data: cf_chains::CcmAdditionalData::try_from(
						metadata.ccm_additional_data.0 .0.to_vec(),
					)
					.expect("Runtime ccm_additional_data exceeds 3,000 bytes"),
				}
			}),
			max_oracle_price_slippage: value.max_oracle_price_slippage,
		}
	}
}

impl<T> From<cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T>>
	for cf_amm::common::PoolPairsMap<T>
{
	fn from(value: cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T>) -> Self {
		Self { base: value.base, quote: value.quote }
	}
}

impl<T> From<cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease<T>>
	for cf_traits::IncreaseOrDecrease<T>
{
	fn from(
		value: cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease<T>,
	) -> Self {
		match value {
			cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease::Increase(t) =>
				cf_traits::IncreaseOrDecrease::Increase(t),
			cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease::Decrease(t) =>
				cf_traits::IncreaseOrDecrease::Decrease(t),
		}
	}
}

impl
	From<
		cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease<
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange,
		>,
	> for pallet_cf_pools::IncreaseOrDecrease<pallet_cf_pools::RangeOrderChange>
{
	fn from(
		value: cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease<
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange,
		>,
	) -> Self {
		match value {
			cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease::Increase(t) =>
				pallet_cf_pools::IncreaseOrDecrease::Increase(pallet_cf_pools::RangeOrderChange{
					liquidity: t.liquidity,
					amounts: t.amounts.into(),
				}),
			cf_static_runtime::runtime_types::cf_traits::liquidity::IncreaseOrDecrease::Decrease(t) =>
				pallet_cf_pools::IncreaseOrDecrease::Decrease(pallet_cf_pools::RangeOrderChange {
					liquidity: t.liquidity,
					amounts: t.amounts.into(),
				}),
		}
	}
}

impl From<cf_static_runtime::runtime_types::sp_runtime::DispatchError>
	for sp_runtime::DispatchError
{
	fn from(error: cf_static_runtime::runtime_types::sp_runtime::DispatchError) -> Self {
		match error {
			cf_static_runtime::runtime_types::sp_runtime::DispatchError::Other =>
				sp_runtime::DispatchError::Other("Other error"),

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::CannotLookup =>
				sp_runtime::DispatchError::CannotLookup,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::BadOrigin =>
				sp_runtime::DispatchError::BadOrigin,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::Module(module_error) =>
				sp_runtime::DispatchError::Module(sp_runtime::ModuleError {
					index: module_error.index,
					error: module_error.error,
					message: None,
				}),

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::ConsumerRemaining =>
				sp_runtime::DispatchError::ConsumerRemaining,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::NoProviders =>
				sp_runtime::DispatchError::NoProviders,

			cf_static_runtime::runtime_types::sp_runtime::DispatchError::Token(token_error) =>
				sp_runtime::DispatchError::Token(match token_error {
					cf_static_runtime::runtime_types::sp_runtime::TokenError::FundsUnavailable =>
						sp_runtime::TokenError::FundsUnavailable,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::OnlyProvider =>
						sp_runtime::TokenError::OnlyProvider,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::BelowMinimum =>
						sp_runtime::TokenError::BelowMinimum,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::CannotCreate =>
						sp_runtime::TokenError::CannotCreate,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::UnknownAsset =>
						sp_runtime::TokenError::UnknownAsset,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::Frozen =>
						sp_runtime::TokenError::Frozen,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::Unsupported =>
						sp_runtime::TokenError::Unsupported,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::CannotCreateHold =>
						sp_runtime::TokenError::CannotCreateHold,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::NotExpendable =>
						sp_runtime::TokenError::NotExpendable,
					cf_static_runtime::runtime_types::sp_runtime::TokenError::Blocked =>
						sp_runtime::TokenError::Blocked,
				}),

			_ => sp_runtime::DispatchError::Other("Unknown error"),
		}
	}
}

impl From<cf_static_runtime::system::events::extrinsic_success::DispatchInfo>
	for frame_system::DispatchEventInfo
{
	fn from(info: cf_static_runtime::system::events::extrinsic_success::DispatchInfo) -> Self {
		Self {
			weight: frame_support::weights::Weight::from_parts(info.weight.ref_time, info.weight.proof_size),
			class: match info.class {
				cf_static_runtime::runtime_types::frame_support::dispatch::DispatchClass::Normal =>
					frame_support::dispatch::DispatchClass::Normal,
				cf_static_runtime::runtime_types::frame_support::dispatch::DispatchClass::Operational =>
					frame_support::dispatch::DispatchClass::Operational,
				cf_static_runtime::runtime_types::frame_support::dispatch::DispatchClass::Mandatory =>
					frame_support::dispatch::DispatchClass::Mandatory,
			},
			pays_fee: match info.pays_fee {
				cf_static_runtime::runtime_types::frame_support::dispatch::Pays::Yes =>
					frame_support::dispatch::Pays::Yes,
				cf_static_runtime::runtime_types::frame_support::dispatch::Pays::No =>
					frame_support::dispatch::Pays::No,
			},
		}
	}
}
