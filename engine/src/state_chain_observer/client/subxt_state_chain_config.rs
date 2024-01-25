use subxt::{config::signed_extensions, Config};

pub enum StateChainConfig {}

impl Config for StateChainConfig {
	// We cannot use our own Runtime's types for every associated type here, see comments below.
	type Hash = <state_chain_runtime::Runtime as frame_system::Config>::Hash;
	type AccountId = subxt::utils::AccountId32; // Requires EncodeAsType trait (which our AccountId doesn't)
	type Address = subxt::utils::MultiAddress<Self::AccountId, ()>; // Must be convertible from Self::AccountId
	type Signature = state_chain_runtime::Signature;
	type Hasher = subxt::ext::sp_runtime::traits::BlakeTwo256; // Requires subxt's custom Hash trait
	type Header = subxt::ext::sp_runtime::generic::Header<u32, Self::Hasher>; // Requires subxt's custom Header trait
	type ExtrinsicParams = signed_extensions::AnyOf<
		Self,
		(
			signed_extensions::CheckSpecVersion,
			signed_extensions::CheckTxVersion,
			signed_extensions::CheckNonce,
			signed_extensions::CheckGenesis<Self>,
			signed_extensions::CheckMortality<Self>,
			signed_extensions::ChargeAssetTxPayment,
			signed_extensions::ChargeTransactionPayment,
		),
	>;
}
