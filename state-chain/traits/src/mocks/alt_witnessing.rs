use super::MockPallet;
use crate::{mocks::MockPalletStorage, InitiateSolanaAltWitnessing};
use cf_chains::sol::SolAddress;

pub struct MockAltWitnessing;

impl MockPallet for MockAltWitnessing {
	const PREFIX: &'static [u8] = b"MockAltWitnessing";
}

const ALT_WITNESSING: &[u8] = b"ALT_WITNESSING";

impl MockAltWitnessing {
	pub fn get_witnessing_alts() -> Vec<SolAddress> {
		Self::get_value(ALT_WITNESSING).unwrap_or_default()
	}
}

impl InitiateSolanaAltWitnessing for MockAltWitnessing {
	fn initiate_alt_witnessing(new: Vec<SolAddress>) {
		Self::mutate_value::<Vec<SolAddress>, _, _>(ALT_WITNESSING, |maybe_alts| {
			let alts = maybe_alts.get_or_insert_default();
			alts.extend(new);
		});
	}
}
