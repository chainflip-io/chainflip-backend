mod chain_tracking;
pub mod deposits;
pub mod source;
pub mod vault_swaps;

use crate::btc::rpc::VerboseTransaction;
use bitcoin::{hashes::Hash, BlockHash};
use cf_chains::btc::{self, deposit_address::DepositAddress, BlockNumber, CHANGE_ADDRESS_SALT};
use cf_primitives::EpochIndex;
use futures_core::Future;

use super::common::{chain_source::Header, epoch_source::Vault};

pub async fn process_egress<ProcessCall, ProcessingFut, ExtraInfo, ExtraHistoricInfo>(
	epoch: Vault<cf_chains::Bitcoin, ExtraInfo, ExtraHistoricInfo>,
	header: Header<u64, BlockHash, (Vec<VerboseTransaction>, Vec<(btc::Hash, BlockNumber)>)>,
	process_call: ProcessCall,
) where
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let (txs, monitored_tx_hashes) = header.data;

	let monitored_tx_hashes = monitored_tx_hashes.iter().map(|(tx_hash, _)| tx_hash);

	for (tx_hash, tx) in success_witnesses(monitored_tx_hashes, txs) {
		process_call(
			state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
				pallet_cf_broadcast::Call::transaction_succeeded {
					tx_out_id: tx_hash,
					signer_id: DepositAddress::new(epoch.info.0.current, CHANGE_ADDRESS_SALT)
						.script_pubkey(),
					tx_fee: tx.fee.unwrap_or_default().to_sat(),
					tx_metadata: (),
					transaction_ref: tx_hash,
				},
			),
			epoch.index,
		)
		.await;
	}
}

fn success_witnesses<'a>(
	monitored_tx_hashes: impl Iterator<Item = &'a btc::Hash> + Clone,
	txs: Vec<VerboseTransaction>,
) -> Vec<(btc::Hash, VerboseTransaction)> {
	let mut successful_witnesses = Vec::new();

	for tx in txs {
		let mut monitored = monitored_tx_hashes.clone();
		let tx_hash = tx.txid.to_byte_array().into();

		if monitored.any(|&monitored_hash| monitored_hash == tx_hash) {
			successful_witnesses.push((tx_hash, tx));
		}
	}
	successful_witnesses
}

#[cfg(test)]
mod tests {

	use bitcoin::Amount;

	use super::*;
	use crate::witness::btc::deposits::tests::{fake_transaction, fake_verbose_vouts};

	#[test]
	fn witnesses_tx_hash_successfully() {
		const FEE_0: u64 = 1;
		const FEE_1: u64 = 111;
		const FEE_2: u64 = 222;
		const FEE_3: u64 = 333;
		let txs = vec![
			fake_transaction(vec![], Some(Amount::from_sat(FEE_0))),
			fake_transaction(
				fake_verbose_vouts(vec![(2324, &DepositAddress::new([0; 32], 0))]),
				Some(Amount::from_sat(FEE_1)),
			),
			fake_transaction(
				fake_verbose_vouts(vec![(232232, &DepositAddress::new([32; 32], 0))]),
				Some(Amount::from_sat(FEE_2)),
			),
			fake_transaction(
				fake_verbose_vouts(vec![(232232, &DepositAddress::new([32; 32], 0))]),
				Some(Amount::from_sat(FEE_3)),
			),
		];

		let tx_hashes = txs.iter().map(|tx| tx.txid.to_byte_array().into()).collect::<Vec<_>>();

		// we're not monitoring for index 2, and they're out of order.
		let monitored_hashes = [tx_hashes[3], tx_hashes[0], tx_hashes[1]];

		let sorted_monitored_hashes = vec![tx_hashes[0], tx_hashes[1], tx_hashes[3]];

		let (success_witness_hashes, txs): (Vec<_>, Vec<_>) =
			success_witnesses(monitored_hashes.iter(), txs).into_iter().unzip();
		assert_eq!(sorted_monitored_hashes, success_witness_hashes);
		assert_eq!(txs[0].fee.unwrap().to_sat(), FEE_0);
		assert_eq!(txs[1].fee.unwrap().to_sat(), FEE_1);
		// we weren't monitoring for 2, so the last fee should be FEE_3.
		assert_eq!(txs[2].fee.unwrap().to_sat(), FEE_3);
	}
}
