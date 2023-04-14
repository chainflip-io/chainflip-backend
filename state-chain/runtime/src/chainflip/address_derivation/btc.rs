use super::AddressDerivation;
use crate::{BitcoinVault, Environment, Validator};
use cf_chains::{
	address::{BitcoinAddressData, BitcoinAddressFor, BitcoinAddressSeed},
	Bitcoin, Chain,
};
use cf_primitives::{chains::assets::btc, IntentId};
use cf_traits::{AddressDerivationApi, EpochInfo};
use sp_runtime::DispatchError;

impl AddressDerivationApi<Bitcoin> for AddressDerivation {
	fn generate_address(
		_ingress_asset: btc::Asset,
		intent_id: IntentId,
	) -> Result<<Bitcoin as Chain>::ChainAccount, DispatchError> {
		// We don't expect to hit this case in the wild because we reuse addresses.
		if intent_id > u32::MAX.into() {
			return Err(DispatchError::Other("Intent ID is too large for BTC address derivation"))
		}

		Ok(BitcoinAddressData {
			address_for: BitcoinAddressFor::Ingress(BitcoinAddressSeed {
				pubkey_x: BitcoinVault::vaults(Validator::epoch_index())
					.ok_or(DispatchError::Other("No vault for epoch"))?
					.public_key
					.pubkey_x,
				salt: intent_id.try_into().unwrap(),
			}),
			network: Environment::bitcoin_network(),
		})
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
					pubkey_x: hex_literal::hex!(
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
