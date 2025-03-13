use super::AddressDerivation;
use crate::{BitcoinThresholdSigner, Environment, EpochKey, String};
use cf_chains::{
	address::{AddressDerivationApi, AddressDerivationError, EncodedAddress},
	btc::{deposit_address::DepositAddress, AggKey, ScriptPubkey},
	Bitcoin, Chain,
};
use cf_primitives::ChannelId;
use cf_traits::KeyProvider;

pub struct BitcoinPrivateBrokerDepositAddresses<Address> {
	pub previous: Option<Address>,
	pub current: Address,
}

impl<A> BitcoinPrivateBrokerDepositAddresses<A> {
	pub fn map_address<B, F>(self, f: F) -> BitcoinPrivateBrokerDepositAddresses<B>
	where
		F: Fn(A) -> B,
	{
		BitcoinPrivateBrokerDepositAddresses {
			previous: self.previous.map(&f),
			current: f(self.current),
		}
	}
}

impl BitcoinPrivateBrokerDepositAddresses<ScriptPubkey> {
	pub fn with_encoded_addresses(self) -> BitcoinPrivateBrokerDepositAddresses<EncodedAddress> {
		self.map_address(|script_pubkey| {
			EncodedAddress::from_chain_account::<Bitcoin>(
				script_pubkey,
				Environment::network_environment(),
			)
		})
	}

	pub fn current_address(&self) -> String {
		self.current.to_address(&Environment::network_environment().into())
	}
}

impl AddressDerivationApi<Bitcoin> for AddressDerivation {
	fn generate_address(
		source_asset: <Bitcoin as Chain>::ChainAsset,
		channel_id: ChannelId,
	) -> Result<<Bitcoin as Chain>::ChainAccount, AddressDerivationError> {
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
		AddressDerivationError,
	> {
		let channel_id: u32 = channel_id
			.try_into()
			.map_err(|_| AddressDerivationError::BitcoinChannelIdTooLarge)?;

		let channel_state = DepositAddress::new(
			// TODO: The key should be passed as an argument (or maybe KeyProvider type arg).
			BitcoinThresholdSigner::active_epoch_key()
				.ok_or(AddressDerivationError::MissingBitcoinVault)?
				.key
				.current,
			channel_id,
		);

		Ok((channel_state.script_pubkey(), channel_state))
	}
}

/// ONLY FOR USE IN RPC CALLS.
///
/// Derives the current and previous BTC vault deposit addresses from the private channel id.
/// Note: This function will **panic** if the private channel id is out of bounds or if there is
/// no active epoch key for Bitcoin.
///
/// Note: If there is no previous key, we return `None` for the previous address.
pub fn derive_btc_vault_deposit_addresses(
	private_channel_id: u64,
) -> BitcoinPrivateBrokerDepositAddresses<ScriptPubkey> {
	let EpochKey { key: AggKey { previous, current }, .. } =
		BitcoinThresholdSigner::active_epoch_key()
			.expect("We should always have a key for the current epoch.");

	let private_channel_id: u32 =
		private_channel_id.try_into().expect("Private channel id out of bounds.");

	BitcoinPrivateBrokerDepositAddresses { previous, current }
		.map_address(|key| DepositAddress::new(key, private_channel_id).script_pubkey())
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Bitcoin;
	use cf_primitives::chains::assets::btc;
	use cf_utilities::assert_ok;
	use pallet_cf_threshold_signature::{CurrentKeyEpoch, Keys};
	use pallet_cf_validator::CurrentEpoch;

	sp_io::TestExternalities::new_empty().execute_with(|| {
		CurrentEpoch::<Runtime>::set(1);
		Keys::<Runtime, crate::BitcoinInstance>::insert(
			1,
			cf_chains::btc::AggKey {
				previous: None,
				current: hex_literal::hex!(
					"9fe94d03955ff4cc5dec97fa5f0dc564ae5ab63012e76dbe84c87c1c83460b48"
				),
			},
		);
		CurrentKeyEpoch::<Runtime, crate::BitcoinInstance>::put(1);
		assert_ok!(<AddressDerivation as AddressDerivationApi<Bitcoin>>::generate_address(
			btc::Asset::Btc,
			1
		));
	});
}
