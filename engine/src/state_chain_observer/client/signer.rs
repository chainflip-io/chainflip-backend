use sp_core::Pair;
use sp_runtime::traits::{IdentifyAccount, Verify};
use state_chain_runtime::{AccountId, Signature};

/// A wrapper around a substrate [`Pair`] that can be used for signing.
#[derive(Clone, Debug)]
pub struct PairSigner<P: Pair> {
	pub account_id: AccountId,
	signer: P,
}

impl<P> PairSigner<P>
where
	Signature: From<P::Signature>,
	<Signature as Verify>::Signer: From<P::Public> + IdentifyAccount<AccountId = AccountId>,
	P: Pair,
{
	/// Creates a new [`Signer`] from a [`Pair`].
	pub fn new(signer: P) -> Self {
		let account_id = <Signature as Verify>::Signer::from(signer.public()).into_account();
		Self { account_id, signer }
	}

	/// Takes a signer payload for an extrinsic, and returns a signature based on it.
	pub fn sign(&self, signer_payload: &[u8]) -> Signature {
		self.signer.sign(signer_payload).into()
	}
}
