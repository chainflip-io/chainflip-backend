use super::*;
use crate::DepositTracker;
use sp_std::collections::btree_set::BTreeSet;

#[derive(Clone, Default, RuntimeDebug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BitcoinDepositTracker {
	deposit_utxos: BTreeSet<Utxo>,
	change_utxos: BTreeSet<Utxo>,
}

impl DepositTracker<Bitcoin> for BitcoinDepositTracker {
	fn total(&self) -> <Bitcoin as Chain>::ChainAmount {
		self.deposit_utxos
			.iter()
			.map(|u| u.amount)
			.sum::<<Bitcoin as Chain>::ChainAmount>() +
			self.change_utxos
				.iter()
				.map(|u| u.amount)
				.sum::<<Bitcoin as Chain>::ChainAmount>()
	}

	fn register_deposit(
		&mut self,
		amount: <Bitcoin as Chain>::ChainAmount,
		deposit_details: &<Bitcoin as Chain>::DepositDetails,
		deposit_channel: &DepositChannel<Bitcoin>,
	) {
		self.deposit_utxos.insert(Utxo {
			amount,
			id: deposit_details.clone(),
			deposit_address: deposit_channel.state.clone(),
		});
	}

	fn register_transfer(&mut self, amount: <Bitcoin as Chain>::ChainAmount) {
		todo!()
	}

	fn mark_as_fetched(&mut self, amount: <Bitcoin as Chain>::ChainAmount) {
		todo!()
	}
}
