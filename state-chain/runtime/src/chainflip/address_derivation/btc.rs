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
					.unwrap_or(Err(DispatchError::Other("No vault for epoch"))?)
					.public_key
					.0,
				salt: intent_id.try_into().unwrap(),
			}),
			network: Environment::bitcoin_network(),
		})
	}
}
