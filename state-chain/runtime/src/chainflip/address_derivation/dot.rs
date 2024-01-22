use crate::Vec;
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	dot::PolkadotAccountId,
	Chain, Polkadot,
};
use cf_primitives::ChannelId;
use cf_utilities::SliceToArray;
use frame_support::sp_runtime::traits::{BlakeTwo256, Hash};
use sp_std::mem::size_of;

use crate::Environment;

use super::AddressDerivation;

impl AddressDerivationApi<Polkadot> for AddressDerivation {
	fn generate_address(
		_source_asset: <Polkadot as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<<Polkadot as Chain>::ChainAccount, AddressDerivationError> {
		const PREFIX: &[u8; 16] = b"modlpy/utilisuba";
		const RAW_PUBLIC_KEY_SIZE: usize = 32;
		const PAYLOAD_LENGTH: usize = PREFIX.len() + RAW_PUBLIC_KEY_SIZE + size_of::<u16>();

		let master_account = Environment::polkadot_vault_account()
			.ok_or(AddressDerivationError::MissingPolkadotVault)?;

		let mut layers = channel_id
			.to_be_bytes()
			.chunks(2)
			.map(|chunk| u16::from_be_bytes(chunk.as_array::<2>()))
			.skip_while(|layer| *layer == 0u16)
			.collect::<Vec<u16>>();

		layers.reverse();

		let payload_hash =
			layers.into_iter().fold(*master_account.aliased_ref(), |sub_account, salt| {
				let mut payload = Vec::with_capacity(PAYLOAD_LENGTH);
				// Fill the first slots with the derivation prefix.
				payload.extend(PREFIX);
				// Then add the 32-byte public key.
				payload.extend(sub_account);
				// Finally, add the index to the end of the payload.
				payload.extend(&salt.to_le_bytes());

				// Hash the whole thing
				BlakeTwo256::hash(&payload).to_fixed_bytes()
			});

		Ok(PolkadotAccountId::from_aliased(payload_hash))
	}

	fn generate_address_and_state(
		source_asset: <Polkadot as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Polkadot as Chain>::ChainAccount, <Polkadot as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((
			<Self as AddressDerivationApi<Polkadot>>::generate_address(source_asset, channel_id)?,
			Default::default(),
		))
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::Runtime;
	use cf_primitives::chains::assets::dot;
	use frame_support::sp_runtime::app_crypto::Ss58Codec;
	use pallet_cf_environment::PolkadotVaultAccountId;

	#[test]
	fn single_layer() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			let (account_id, address_format) =
				sp_runtime::AccountId32::from_ss58check_with_version(
					"15uPkKV7SsNXxw5VCu3LgnuaR5uSZ4QMyzxnLfDFE9J5nni9",
				)
				.unwrap();
			PolkadotVaultAccountId::<Runtime>::put(PolkadotAccountId::from_aliased(
				*account_id.as_ref(),
			));

			assert_eq!(
				"12AeXofJkQErqQuiJmJapqwS4KiAZXBhAYoj9HZ2sYo36mRg",
				sp_runtime::AccountId32::new(
					*<AddressDerivation as AddressDerivationApi<Polkadot>>::generate_address(
						dot::Asset::Dot,
						6259
					)
					.unwrap()
					.aliased_ref()
				)
				.to_ss58check_with_version(address_format),
			);
		});
	}

	#[test]
	fn four_layers() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			let (alice, address_format) = sp_runtime::AccountId32::from_ss58check_with_version(
				"15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5",
			)
			.unwrap();
			PolkadotVaultAccountId::<Runtime>::put(PolkadotAccountId::from_aliased(
				*alice.as_ref(),
			));

			assert_eq!(
				// The following account was generated using nested utility.asDerivative calls in
				// PolkaJS. The wrapped call was system.remarkWithEvent, which emits the generated
				// address in its event.
				//
				// The call was dispatched from the Alice account.
				//
				// To see the call go to
				// extrinsics/decode/0x1a0101001a0102001a0103001a01040000071448414c4c4f
				// on any polkaJS instance connected to a polkadot node.
				//
				// Call details:
				// 1a01 is the utility.asDerivative call index.
				// 0007 is the system.remarkWithEvent call index.
				// Encoded call: 0x1a01 0100 1a01 0200 1a01 0300 1a01 0400 0007 1448414c4c4f
				//                 ---- -^-- ---- -^-- ---- -^-- ---- -^--      b"HALLO"
				"1422Jc2BYRh5ENjxWJchoHPSC2Rd4jFs8PDWHqBJue4yskEt",
				sp_runtime::AccountId32::new(
					*<AddressDerivation as AddressDerivationApi<Polkadot>>::generate_address(
						dot::Asset::Dot,
						0x0004_0003_0002_0001
					)
					.unwrap()
					.aliased_ref()
				)
				.to_ss58check_with_version(address_format),
			);
		});
	}
}
