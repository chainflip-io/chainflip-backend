use crate::subxt_state_chain_config::StateChainConfig;
use sp_runtime::traits::{IdentifyAccount, Verify};
use state_chain_runtime::{AccountId, Signature};

pub mod broker_crypto {
	use sp_application_crypto::{app_crypto, sr25519, KeyTypeId};
	/// Broker Key Type ID used to store the key on state chain node keystore
	pub const BROKER_ID_KEY: KeyTypeId = KeyTypeId(*b"brok");

	app_crypto!(sr25519, BROKER_ID_KEY);
}

pub struct SubxtSignerInterface<T>(subxt::utils::AccountId32, T);

impl<P> SubxtSignerInterface<P>
where
	Signature: From<P::Signature>,
	<Signature as Verify>::Signer: From<P::Public> + IdentifyAccount<AccountId = AccountId>,
	P: sp_core::Pair,
{
	pub fn new(pair: P) -> Self {
		let account_id = <Signature as Verify>::Signer::from(pair.public()).into_account();
		let bytes: &[u8; 32] = account_id.as_ref();
		Self(subxt::utils::AccountId32::from(*bytes), pair)
	}
}

impl subxt::tx::Signer<StateChainConfig> for SubxtSignerInterface<sp_core::sr25519::Pair> {
	fn account_id(&self) -> <StateChainConfig as subxt::Config>::AccountId {
		self.0.clone()
	}

	fn address(&self) -> <StateChainConfig as subxt::Config>::Address {
		subxt::utils::MultiAddress::Id(self.0.clone())
	}

	fn sign(&self, bytes: &[u8]) -> <StateChainConfig as subxt::Config>::Signature {
		use sp_core::Pair;
		state_chain_runtime::Signature::Sr25519(self.1.sign(bytes))
	}
}
