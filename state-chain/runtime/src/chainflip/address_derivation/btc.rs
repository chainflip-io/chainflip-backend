use super::AddressDerivation;
use crate::{BitcoinVault, Validator};
use cf_chains::{
	btc::{deposit_address::DepositAddress, BitcoinFetchId, ScriptPubkey},
	Bitcoin, Chain,
};
use cf_primitives::{chains::assets::btc, ChannelId};
use cf_traits::{AddressDerivationApi, DepositChannel, EpochInfo};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_runtime::DispatchError;

impl AddressDerivationApi<Bitcoin> for AddressDerivation {
	fn generate_address(
		_source_asset: btc::Asset,
		channel_id: ChannelId,
	) -> Result<<Bitcoin as Chain>::ChainAccount, DispatchError> {
		// We don't expect to hit this case in the wild because we reuse addresses.
		let channel_id: u32 = channel_id
			.try_into()
			.map_err(|_| "Intent ID is too large for BTC address derivation")?;

		Ok(DepositAddress::new(
			BitcoinVault::vaults(Validator::epoch_index())
				.ok_or(DispatchError::Other("No vault for epoch"))?
				.public_key
				.current,
			channel_id,
		)
		.script_pubkey())
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
pub struct BitcoinDepositAddress {
	pub channel_id: u64,
	pub address: ScriptPubkey,
	pub asset: btc::Asset,
}

impl DepositChannel<Bitcoin> for BitcoinDepositAddress {
	type Address = ScriptPubkey;
	type DepositFetchId = BitcoinFetchId;
	type AddressDerivation = AddressDerivation;

	fn new(
		channel_id: ChannelId,
		asset: <Bitcoin as Chain>::ChainAsset,
	) -> Result<Self, DispatchError>
	where
		Self: Sized,
	{
		let address = <AddressDerivation as AddressDerivationApi<Bitcoin>>::generate_address(
			asset, channel_id,
		)?;
		Ok(Self { address, asset, channel_id })
	}

	fn get_address(&self) -> Self::Address {
		self.address.clone()
	}

	fn get_fetch_id(&self) -> Option<Self::DepositFetchId> {
		Some(BitcoinFetchId(self.channel_id))
	}

	fn get_channel_id(&self) -> ChannelId {
		self.channel_id
	}

	fn get_asset(&self) -> <Bitcoin as Chain>::ChainAsset {
		self.asset
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Bitcoin;
	use pallet_cf_validator::CurrentEpoch;
	use pallet_cf_vaults::{Vault, Vaults};

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
		assert!(<AddressDerivation as AddressDerivationApi<Bitcoin>>::generate_address(
			btc::Asset::Btc,
			1
		)
		.is_ok());
	});
}
