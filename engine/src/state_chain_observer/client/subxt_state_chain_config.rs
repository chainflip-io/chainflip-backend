use subxt::{config::signed_extensions, Config};

pub enum StateChainConfig {}

impl Config for StateChainConfig {
	type Hash = <state_chain_runtime::Runtime as frame_system::Config>::Hash;
	type AccountId = subxt::utils::AccountId32;
	type Address = subxt::utils::MultiAddress<Self::AccountId, ()>;
	type Signature = subxt::utils::MultiSignature;
	type Hasher = subxt::ext::sp_runtime::traits::BlakeTwo256;
	type Header = subxt::ext::sp_runtime::generic::Header<u32, Self::Hasher>;
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
