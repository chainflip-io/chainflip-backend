use sp_runtime::DispatchError;
use subxt::{config::signed_extensions, Config};

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
	type ExtrinsicParams = signed_extensions::AnyOf<
		Self,
		(
			signed_extensions::CheckSpecVersion,
			signed_extensions::CheckTxVersion,
			signed_extensions::CheckNonce,
			signed_extensions::CheckGenesis<Self>,
			signed_extensions::CheckMortality<Self>,
			signed_extensions::ChargeAssetTxPayment<Self>,
			signed_extensions::ChargeTransactionPayment,
			signed_extensions::CheckMetadataHash,
		),
	>;
}

/// This macro generates a strongly typed API from a WASM file. All types are substituted with
/// corresponding types that implement some traits, allowing subxt to scale encode/decode these
/// types. However, this makes it challenging to convert from the new generated types to cf types,
/// especially for hierarchical types. The trick is use the `substitute_type` directive to instruct
/// the subxt macro to use certain types in place of the default generated types. Example:
/// ```ignore
/// substitute_type(path = "cf_chains::ChannelRefundParametersGeneric<A>", with = "::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>>")
/// ```
/// * This will generate: ::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>> in
///   place of the default runtime_types::cf_chains::ChannelRefundParametersGeneric<A>
/// * The `::subxt::utils::Static` is required to wrap the type and implement the necessary
///   `EncodeAsType` and `DecodeAsType` traits.
/// * Any cf type that needs to be substituted must be defined in the `substitute_type` directive.
/// * For any other complex types with multiple hieracrchies or generics, please add manual
///   conversion functions below.
#[subxt::subxt(
	runtime_path = "../../target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm",
	substitute_type(
		path = "cf_chains::address::EncodedAddress",
		with = "::subxt::utils::Static<cf_chains::address::EncodedAddress>"
	),
	substitute_type(
		path = "cf_primitives::chains::assets::any::Asset",
		with = "::subxt::utils::Static<cf_primitives::chains::assets::any::Asset>"
	),
	substitute_type(
		path = "cf_primitives::chains::ForeignChain",
		with = "::subxt::utils::Static<cf_primitives::chains::ForeignChain>"
	),
	substitute_type(
		path = "cf_amm::common::Side",
		with = "::subxt::utils::Static<cf_amm::common::Side>"
	),
	substitute_type(
		path = "cf_chains::ChannelRefundParametersGeneric<A>",
		with = "::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>>"
	)
)]
pub mod cf_static_runtime {}

// Conversions from cf_static_runtime::runtime_types

impl<T> From<cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T>>
	for cf_amm::common::PoolPairsMap<T>
{
	fn from(value: cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T>) -> Self {
		Self { base: value.base, quote: value.quote }
	}
}

impl<T> From<cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T>>
	for pallet_cf_pools::pallet::IncreaseOrDecrease<T>
{
	fn from(
		value: cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T>,
	) -> Self {
		match value {
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t),
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t),
		}
	}
}

impl
	From<
		cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange,
		>,
	> for pallet_cf_pools::pallet::IncreaseOrDecrease<pallet_cf_pools::RangeOrderChange>
{
	fn from(
		value: cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange,
		>,
	) -> Self {
		match value {
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(pallet_cf_pools::RangeOrderChange{
				liquidity: t.liquidity,
				amounts: t.amounts.into(),
			}),
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(pallet_cf_pools::RangeOrderChange {
				liquidity: t.liquidity,
				amounts: t.amounts.into(),
			}),
		}
	}
}

impl From<cf_static_runtime::runtime_types::sp_runtime::DispatchError> for DispatchError {
	fn from(error: cf_static_runtime::runtime_types::sp_runtime::DispatchError) -> Self {
		match error {
			// TODO: investigate why the types are not symmetrical. may be subxt-cli version
			// mismatch
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
