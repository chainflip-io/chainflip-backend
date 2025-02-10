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
