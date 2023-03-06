use cf_chains::{btc::ingress_address::derive_btc_ingress_address, Bitcoin, Chain};
use cf_primitives::{chains::assets::btc, IntentId};
use cf_traits::AddressDerivationApi;
use sp_runtime::DispatchError;

use super::AddressDerivation;

use crate::Runtime;

impl AddressDerivationApi<Bitcoin> for AddressDerivation {
	fn generate_address(
		_ingress_asset: btc::Asset,
		intent_id: IntentId,
	) -> Result<<Bitcoin as Chain>::ChainAccount, DispatchError> {
		// We don't expect to hit this case in the wild because we reuse addresses.
		if intent_id > u32::MAX.into() {
			return Err(DispatchError::Other("Intent ID is too large for BTC address derivation"))
		}
		let _btc_address = derive_btc_ingress_address(
			[0x0; 32],
			intent_id.try_into().unwrap(),
			<Runtime as pallet_cf_environment::Config>::BitcoinNetwork::get(),
		);

		todo!("Get the actual pubkey_x once the BTC vault pallet is instantiated");
	}
}
