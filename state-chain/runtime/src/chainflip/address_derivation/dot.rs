use crate::Vec;
use cf_chains::{Chain, Polkadot};
use cf_primitives::{chains::assets::dot, IntentId};
use cf_traits::AddressDerivationApi;
use sp_core::crypto::{AccountId32, ByteArray};
use sp_runtime::{
	traits::{BlakeTwo256, Hash},
	DispatchError,
};
use sp_std::mem::size_of;

use crate::Environment;

use super::AddressDerivation;

impl AddressDerivationApi<Polkadot> for AddressDerivation {
	fn generate_address(
		_ingress_asset: dot::Asset,
		intent_id: IntentId,
	) -> Result<<Polkadot as Chain>::ChainAccount, DispatchError> {
		const PREFIX: &[u8; 16] = b"modlpy/utilisuba";
		const RAW_PUBLIC_KEY_SIZE: usize = 32;
		const PAYLOAD_LENGTH: usize = PREFIX.len() + RAW_PUBLIC_KEY_SIZE + size_of::<u16>();

		let master_account = Environment::polkadot_vault_account()
			.ok_or(DispatchError::Other("Vault Account does not exist."))?;

		// Because we re-use addresses, we don't expect to hit this case in the wild.
		if intent_id > u16::MAX.into() {
			return Err(DispatchError::Other(
				"Intent ID too large. Polkadot can only support up to u16 addresses",
			))
		}

		let mut payload = Vec::with_capacity(PAYLOAD_LENGTH);
		// Fill the first slots with the derivation prefix.
		payload.extend(PREFIX);
		// Then add the 32-byte public key.
		payload.extend(master_account.as_slice());
		// Finally, add the index to the end of the payload.
		payload.extend(&(<u16>::try_from(intent_id).unwrap()).to_le_bytes());

		// Hash the whole thing
		let payload_hash = BlakeTwo256::hash(&payload);

		Ok(AccountId32::from(*payload_hash.as_fixed_bytes()))
	}
}

#[test]
fn test_dot_derive() {
	use crate::Runtime;
	use pallet_cf_environment::PolkadotVaultAccountId;
	use sp_runtime::app_crypto::Ss58Codec;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		let (account_id, address_format) = AccountId32::from_ss58check_with_version(
			"15uPkKV7SsNXxw5VCu3LgnuaR5uSZ4QMyzxnLfDFE9J5nni9",
		)
		.unwrap();
		PolkadotVaultAccountId::<Runtime>::put(account_id);

		assert_eq!(
			"12AeXofJkQErqQuiJmJapqwS4KiAZXBhAYoj9HZ2sYo36mRg",
			<AddressDerivation as AddressDerivationApi<Polkadot>>::generate_address(
				dot::Asset::Dot,
				6259
			)
			.unwrap()
			.to_ss58check_with_version(address_format),
		);
		println!("Derivation worked for DOT! 🚀");
	});
}
