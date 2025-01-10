use crate::Environment;
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError},
	hub::calculate_derived_address,
	Assethub, Chain,
};
use cf_primitives::ChannelId;

use super::AddressDerivation;

impl AddressDerivationApi<Assethub> for AddressDerivation {
	fn generate_address(
		_source_asset: <Assethub as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<<Assethub as Chain>::ChainAccount, AddressDerivationError> {
		let master_account = Environment::assethub_vault_account()
			.ok_or(AddressDerivationError::MissingAssethubVault)?;

		Ok(calculate_derived_address(master_account, channel_id))
	}

	fn generate_address_and_state(
		source_asset: <Assethub as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Assethub as Chain>::ChainAccount, <Assethub as Chain>::DepositChannelState),
		AddressDerivationError,
	> {
		Ok((
			<Self as AddressDerivationApi<Assethub>>::generate_address(source_asset, channel_id)?,
			Default::default(),
		))
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::Runtime;
	use cf_chains::dot::PolkadotAccountId;
	use cf_primitives::chains::assets::hub;
	use frame_support::sp_runtime::app_crypto::Ss58Codec;
	use pallet_cf_environment::AssethubVaultAccountId;

	#[test]
	fn single_layer() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			let (account_id, address_format) =
				sp_runtime::AccountId32::from_ss58check_with_version(
					"15uPkKV7SsNXxw5VCu3LgnuaR5uSZ4QMyzxnLfDFE9J5nni9",
				)
				.unwrap();
			AssethubVaultAccountId::<Runtime>::put(PolkadotAccountId::from_aliased(
				*account_id.as_ref(),
			));

			assert_eq!(
				"12AeXofJkQErqQuiJmJapqwS4KiAZXBhAYoj9HZ2sYo36mRg",
				sp_runtime::AccountId32::new(
					*<AddressDerivation as AddressDerivationApi<Assethub>>::generate_address(
						hub::Asset::HubUsdc,
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
			AssethubVaultAccountId::<Runtime>::put(PolkadotAccountId::from_aliased(
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
					*<AddressDerivation as AddressDerivationApi<Assethub>>::generate_address(
						hub::Asset::HubDot,
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
