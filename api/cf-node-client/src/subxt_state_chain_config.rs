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

use subxt::{config::transaction_extensions, Config};

#[derive(Debug, Clone)]
pub enum StateChainConfig {}

impl Config for StateChainConfig {
	// We cannot use our own Runtime's types for every associated type here, see comments below.
	type Hash = subxt::utils::H256;
	type AccountId = subxt::utils::AccountId32; // Requires EncodeAsType trait (which our AccountId doesn't)
	type Address = subxt::utils::MultiAddress<Self::AccountId, ()>; // Must be convertible from Self::AccountId
	type Signature = state_chain_runtime::Signature;
	type Hasher = subxt::config::substrate::BlakeTwo256;
	type Header = subxt::config::substrate::SubstrateHeader<u32, Self::Hasher>;
	type AssetId = u32; // Not used - we don't use pallet-assets
	type ExtrinsicParams = transaction_extensions::AnyOf<
		Self,
		(
			transaction_extensions::VerifySignature<Self>,
			transaction_extensions::CheckSpecVersion,
			transaction_extensions::CheckTxVersion,
			transaction_extensions::CheckNonce,
			transaction_extensions::CheckGenesis<Self>,
			transaction_extensions::CheckMortality<Self>,
			transaction_extensions::ChargeAssetTxPayment<Self>,
			transaction_extensions::ChargeTransactionPayment,
			transaction_extensions::CheckMetadataHash,
		),
	>;
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
// TODO: To check this change
impl<T>
	From<
		cf_static_runtime::runtime_types::cf_chains::ChannelRefundParametersGeneric<
			T,
			Option<
				cf_static_runtime::runtime_types::cf_chains::CcmChannelMetadata<
					cf_static_runtime::runtime_types::cf_chains::CcmAdditionalData,
				>,
			>,
		>,
	> for cf_chains::ChannelRefundParametersGeneric<T>
{
	fn from(
		value: cf_static_runtime::runtime_types::cf_chains::ChannelRefundParametersGeneric<
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
			min_price: value.min_price.0,
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

impl From<cf_static_runtime::runtime_types::frame_support::dispatch::DispatchInfo>
	for frame_support::dispatch::DispatchInfo
{
	fn from(info: cf_static_runtime::runtime_types::frame_support::dispatch::DispatchInfo) -> Self {
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
