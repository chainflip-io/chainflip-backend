use super::AddressDerivation;
use crate::BitcoinVault;
use cf_chains::{
	address::AddressDerivationApi, btc::deposit_address::DepositAddress, Bitcoin, Chain,
};
use cf_primitives::ChannelId;
use cf_traits::KeyProvider;
use frame_support::sp_runtime::DispatchError;

impl AddressDerivationApi<Bitcoin> for AddressDerivation {
	fn generate_address(
		source_asset: <Bitcoin as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<<Bitcoin as Chain>::ChainAccount, DispatchError> {
		<Self as AddressDerivationApi<Bitcoin>>::generate_address_and_state(
			source_asset,
			channel_id,
		)
		.map(|(address, _)| address)
	}

	fn generate_address_and_state(
		_source_asset: <Bitcoin as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<
		(<Bitcoin as Chain>::ChainAccount, <Bitcoin as Chain>::DepositChannelState),
		DispatchError,
	> {
		let channel_id: u32 = channel_id
			.try_into()
			.map_err(|_| "Intent ID is too large for BTC address derivation")?;

		let channel_state = DepositAddress::new(
			// TODO: The key should be passed as an argument (or maybe KeyProvider type arg).
			BitcoinVault::active_epoch_key()
				.ok_or(DispatchError::Other("No vault for epoch"))?
				.key
				.current,
			channel_id,
		);

		Ok((channel_state.script_pubkey(), channel_state))
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Bitcoin;
	use cf_primitives::chains::assets::btc;
	use cf_traits::KeyState;
	use cf_utilities::assert_ok;
	use pallet_cf_validator::CurrentEpoch;
	use pallet_cf_vaults::{CurrentVaultEpochAndState, Vault, VaultEpochAndState, Vaults};

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		CurrentEpoch::<Runtime>::set(1);
		Vaults::<Runtime, crate::BitcoinInstance>::insert(
			1,
			Vault::<Bitcoin> {
				public_key: cf_chains::btc::AggKey {
					previous: None,
					current: hex_literal::hex!(
						"9fe94d03955ff4cc5dec97fa5f0dc564ae5ab63012e76dbe84c87c1c83460b48"
					),
				},
				active_from_block: 1,
			},
		);
		CurrentVaultEpochAndState::<Runtime, crate::BitcoinInstance>::put(VaultEpochAndState {
			epoch_index: 1,
			key_state: KeyState::Unlocked,
		});
		assert_ok!(<AddressDerivation as AddressDerivationApi<Bitcoin>>::generate_address(
			btc::Asset::Btc,
			1
		));
	});
}
