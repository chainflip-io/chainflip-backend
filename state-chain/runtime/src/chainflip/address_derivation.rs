#[cfg(feature = "ibiza")]
use crate::Vec;
use crate::{Environment, EthEnvironment};
#[cfg(feature = "ibiza")]
use cf_chains::Polkadot;
use cf_chains::{eth::ingress_address::get_create_2_address, Chain, ChainEnvironment, Ethereum};
#[cfg(feature = "ibiza")]
use cf_primitives::chains::assets::dot;
use cf_primitives::{chains::assets::eth, IntentId};
use cf_traits::AddressDerivationApi;
#[cfg(feature = "ibiza")]
use sp_core::crypto::{AccountId32, ByteArray};
#[cfg(feature = "ibiza")]
use sp_runtime::traits::{BlakeTwo256, Hash};
use sp_runtime::DispatchError;
#[cfg(feature = "ibiza")]
use sp_std::mem::size_of;

pub struct AddressDerivation;

impl AddressDerivationApi<Ethereum> for AddressDerivation {
	fn generate_address(
		ingress_asset: eth::Asset,
		intent_id: IntentId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(get_create_2_address(
			ingress_asset,
			Environment::eth_vault_address(),
			match ingress_asset {
				eth::Asset::Eth => None,
				_ => Some(
					EthEnvironment::lookup(ingress_asset)
						.expect("ERC20 asset to be supported!")
						.to_fixed_bytes()
						.to_vec(),
				),
			},
			intent_id,
		)
		.into())
	}
}

#[cfg(feature = "ibiza")]
impl AddressDerivationApi<Polkadot> for AddressDerivation {
	fn generate_address(
		_ingress_asset: dot::Asset,
		intent_id: IntentId,
	) -> Result<<Polkadot as Chain>::ChainAccount, DispatchError> {
		const PREFIX: &[u8; 16] = b"modlpy/utilisuba";
		const PAYLOAD_LENGTH: usize = PREFIX.len() + 32 + size_of::<u16>();

		let master_account = Environment::get_polkadot_vault_account()
			.ok_or(DispatchError::Other("Vault Account does not exist."))?;

		let mut payload = Vec::with_capacity(PAYLOAD_LENGTH);
		// Fill the first slots with the derivation prefix.
		payload.extend(PREFIX);
		// Then add the 32-byte public key.
		payload.extend(master_account.as_slice());
		// Finally, add the index to the end of the payload.
		payload.extend(&(intent_id as u16).to_le_bytes());

		// Hash the whole thing
		let payload_hash = BlakeTwo256::hash(&payload);

		Ok(AccountId32::from(*payload_hash.as_fixed_bytes()))
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Ethereum;
	use cf_primitives::Asset;
	use pallet_cf_environment::EthereumSupportedAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Eth,
			1
		)
		.is_ok());
		// The genesis build is not running, so we have to add it manually
		EthereumSupportedAssets::<Runtime>::insert(Asset::Flip, [0; 20]);
		// Expect address generation to be successfully for ERC20 Flip token
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Flip,
			1
		)
		.is_ok());

		// Address derivation for Dot is currently unimplemented.
		// Expect address generation to return an error for unsupported assets. Because we are
		// running a test gainst ETH the DOT asset will be always unsupported.
		// assert!(AddressDerivation::generate_address(Asset::Dot, 1).is_err());
	});
}

#[cfg(feature = "ibiza")]
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
