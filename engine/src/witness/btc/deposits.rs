use std::collections::HashMap;

use cf_primitives::EpochIndex;
use futures_core::Future;
use itertools::Itertools;
use pallet_cf_ingress_egress::{DepositChannelDetails, DepositWitness};
use state_chain_runtime::BitcoinInstance;

use super::{
	super::common::chunked_chain_source::chunked_by_vault::{
		builder::ChunkedByVaultBuilder, private_deposit_channels::BrokerPrivateChannels,
		ChunkedByVault,
	},
	vault_swaps::BtcIngressEgressCall,
};
use crate::{
	btc::rpc::VerboseTransaction,
	witness::common::{
		chunked_chain_source::chunked_by_vault::deposit_addresses::Addresses, RuntimeCallHasChain,
		RuntimeHasChain,
	},
};
use bitcoin::{hashes::Hash, BlockHash};
use cf_chains::{
	assets::btc,
	btc::{deposit_address::DepositAddress, Utxo, UtxoId},
	Bitcoin,
};

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub fn btc_deposits<ProcessCall, ProcessingFut>(
		self,
		process_call: ProcessCall,
	) -> ChunkedByVaultBuilder<
		impl ChunkedByVault<
			Index = u64,
			Hash = BlockHash,
			Data = Vec<VerboseTransaction>,
			Chain = Bitcoin,
		>,
	>
	where
		Inner: ChunkedByVault<
			Index = u64,
			Hash = BlockHash,
			Data = ((((), Vec<VerboseTransaction>), Addresses<Inner>), BrokerPrivateChannels),
			Chain = Bitcoin,
		>,
		ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
			+ Send
			+ Sync
			+ Clone
			+ 'static,
		ProcessingFut: Future<Output = ()> + Send + 'static,
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		state_chain_runtime::RuntimeCall:
			RuntimeCallHasChain<state_chain_runtime::Runtime, Inner::Chain>,
	{
		self.then(move |epoch, header| {
			let process_call = process_call.clone();
			async move {
				// TODO: Make addresses a Map of some kind?
				let ((((), txs), deposit_channels), private_channels) = header.data;

				let vault_addresses = {
					use cf_chains::btc::{deposit_address::DepositAddress, AggKey};

					let key: &AggKey = &epoch.info.0;

					// Take all current private broker channels and use them to build a list of all
					// deposit addresses that we should check for vault swaps. Note that we
					// monitor previous epoch key (if exists) in addition to the current one, which
					// means we get up to two deposit addresses per broker.
					[key.current].into_iter().chain(key.previous).flat_map(|key| {
						private_channels.clone().into_iter().map(move |(broker_id, channel_id)| {
							(
								broker_id,
								channel_id,
								DepositAddress::new(
									key,
									channel_id.try_into().expect("BTC channel id must fit in u32"),
								),
							)
						})
					})
				};

				for (broker_id, channel_id, vault_address) in vault_addresses {
					for tx in &txs {
						if let Some(deposit) = super::vault_swaps::try_extract_vault_swap_witness(
							tx,
							&vault_address,
							channel_id,
							&broker_id,
						) {
							process_call(
								BtcIngressEgressCall::vault_swap_request {
									block_height: header.index,
									deposit: Box::new(deposit),
								}
								.into(),
								epoch.index,
							)
							.await;
						}
					}
				}

				let deposit_addresses = map_script_addresses(deposit_channels);

				let deposit_witnesses = deposit_witnesses(&txs, &deposit_addresses);

				// Submit all deposit witnesses for the block.
				if !deposit_witnesses.is_empty() {
					process_call(
						pallet_cf_ingress_egress::Call::<_, BitcoinInstance>::process_deposits {
							deposit_witnesses,
							block_height: header.index,
						}
						.into(),
						epoch.index,
					)
					.await;
				}
				txs
			}
		})
	}
}

fn deposit_witnesses(
	txs: &[VerboseTransaction],
	deposit_addresses: &HashMap<Vec<u8>, DepositAddress>,
) -> Vec<DepositWitness<Bitcoin>> {
	txs.iter()
		.flat_map(|tx| {
			Iterator::zip(0.., &tx.vout)
				.filter(|(_vout, tx_out)| tx_out.value.to_sat() > 0)
				.filter_map(|(vout, tx_out)| {
					deposit_addresses.get(tx_out.script_pubkey.as_bytes()).map(|deposit_address| {
						let amount = tx_out.value.to_sat();
						DepositWitness::<Bitcoin> {
							deposit_address: deposit_address.script_pubkey(),
							asset: btc::Asset::Btc,
							amount,
							deposit_details: Utxo {
								id: UtxoId { tx_id: tx.txid.to_byte_array().into(), vout },
								amount,
								deposit_address: deposit_address.clone(),
							},
						}
					})
				})
				.sorted_by_key(|deposit_witness| deposit_witness.deposit_address.clone())
				.chunk_by(|deposit_witness| deposit_witness.deposit_address.clone())
				.into_iter()
				.map(|(_deposit_address, deposit_witnesses)| {
					// We only take the largest output of a tx as a deposit witness. This is to
					// avoid attackers spamming us with many small outputs in a tx. Inputs are more
					// expensive than outputs - thus, the attacker could send many outputs (cheap
					// for them) which results in us needing to sign many *inputs*, expensive for
					// us. sort by descending by amount
					deposit_witnesses.max_by_key(|deposit_witness| deposit_witness.amount).unwrap()
				})
				.collect::<Vec<_>>()
		})
		.collect()
}

fn map_script_addresses(
	deposit_channels: Vec<DepositChannelDetails<state_chain_runtime::Runtime, BitcoinInstance>>,
) -> HashMap<Vec<u8>, DepositAddress> {
	deposit_channels
		.into_iter()
		.map(|channel| {
			assert_eq!(channel.deposit_channel.asset, btc::Asset::Btc);
			let deposit_address = channel.deposit_channel.state;
			let script_pubkey = channel.deposit_channel.address;

			(script_pubkey.bytes(), deposit_address)
		})
		.collect()
}

#[cfg(test)]
pub mod tests {

	use crate::btc::rpc::VerboseTxOut;

	use super::*;
	use bitcoin::{
		absolute::{Height, LockTime},
		Amount, ScriptBuf, Txid,
	};
	use cf_chains::{btc::deposit_address::DepositAddress, DepositChannel};
	use pallet_cf_ingress_egress::{BoostStatus, ChannelAction};
	use rand::{seq::SliceRandom, Rng, SeedableRng};
	use sp_runtime::AccountId32;

	pub fn fake_transaction(tx_outs: Vec<VerboseTxOut>, fee: Option<Amount>) -> VerboseTransaction {
		let random_bytes: [u8; 32] = rand::thread_rng().gen();
		let txid = Txid::from_byte_array(random_bytes);
		VerboseTransaction {
			txid,
			locktime: LockTime::Blocks(Height::ZERO),
			vin: vec![],
			vout: tx_outs,
			fee,
			// not important, we just need to set it to a value.
			hash: txid,
			size: Default::default(),
			vsize: Default::default(),
			weight: Default::default(),
			hex: Default::default(),
		}
	}

	fn fake_details(
		deposit_address: DepositAddress,
	) -> DepositChannelDetails<state_chain_runtime::Runtime, BitcoinInstance> {
		use cf_chains::{btc::ScriptPubkey, ForeignChainAddress};
		DepositChannelDetails::<_, BitcoinInstance> {
			owner: AccountId32::new([0xab; 32]),
			opened_at: 1,
			expires_at: 10,
			deposit_channel: DepositChannel {
				channel_id: 1,
				address: deposit_address.script_pubkey(),
				asset: btc::Asset::Btc,
				state: deposit_address,
			},
			action: ChannelAction::<AccountId32>::LiquidityProvision {
				lp_account: AccountId32::new([0xab; 32]),
				refund_address: Some(ForeignChainAddress::Btc(ScriptPubkey::P2PKH([0; 20]))),
			},
			boost_fee: 0,
			boost_status: BoostStatus::NotBoosted,
		}
	}

	pub fn fake_verbose_vouts(
		amounts_and_addresses: Vec<(u64, &DepositAddress)>,
	) -> Vec<VerboseTxOut> {
		amounts_and_addresses
			.into_iter()
			.enumerate()
			.map(|(n, (amount, address))| VerboseTxOut {
				value: Amount::from_sat(amount),
				n: n as u64,
				script_pubkey: ScriptBuf::from(address.script_pubkey().bytes()),
			})
			.collect()
	}

	#[test]
	fn deposit_witnesses_no_utxos_no_monitored() {
		let txs = vec![fake_transaction(vec![], None), fake_transaction(vec![], None)];
		let deposit_witnesses = deposit_witnesses(&txs, &HashMap::new());
		assert!(deposit_witnesses.is_empty());
	}

	#[test]
	fn filter_out_value_0() {
		let deposit_address = DepositAddress::new([0; 32], 9);

		const UTXO_WITNESSED_1: u64 = 2324;
		let txs = vec![fake_transaction(
			fake_verbose_vouts(vec![(2324, &deposit_address), (0, &deposit_address)]),
			None,
		)];

		let deposit_witnesses =
			deposit_witnesses(&txs, &map_script_addresses(vec![(fake_details(deposit_address))]));
		assert_eq!(deposit_witnesses.len(), 1);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
	}

	#[test]
	fn deposit_witnesses_several_same_tx() {
		const LARGEST_UTXO_TO_DEPOSIT: u64 = 2324;
		const UTXO_TO_DEPOSIT_2: u64 = 1234;
		const UTXO_TO_DEPOSIT_3: u64 = 2000;

		let deposit_address = DepositAddress::new([0; 32], 9);

		let txs = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_TO_DEPOSIT_2, &deposit_address),
					(12223, &DepositAddress::new([0; 32], 10)),
					(LARGEST_UTXO_TO_DEPOSIT, &deposit_address),
					(UTXO_TO_DEPOSIT_3, &deposit_address),
				]),
				None,
			),
			fake_transaction(vec![], None),
		];

		let deposit_witnesses =
			deposit_witnesses(&txs, &map_script_addresses(vec![fake_details(deposit_address)]));
		assert_eq!(deposit_witnesses.len(), 1);
		assert_eq!(deposit_witnesses[0].amount, LARGEST_UTXO_TO_DEPOSIT);
	}

	#[test]
	fn deposit_witnesses_to_different_deposit_addresses_same_tx_is_witnessed() {
		const LARGEST_UTXO_TO_DEPOSIT: u64 = 2324;
		const UTXO_TO_DEPOSIT_2: u64 = 1234;
		const UTXO_FOR_SECOND_DEPOSIT: u64 = 2000;

		let deposit_address_1 = DepositAddress::new([0; 32], 9);
		let deposit_address_2 = DepositAddress::new([0; 32], 1232);

		let txs = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_TO_DEPOSIT_2, &deposit_address_1),
					(12223, &DepositAddress::new([0; 32], 999)),
					(LARGEST_UTXO_TO_DEPOSIT, &deposit_address_1),
					(UTXO_FOR_SECOND_DEPOSIT, &deposit_address_2),
				]),
				None,
			),
			fake_transaction(vec![], None),
		];

		let deposit_witnesses = deposit_witnesses(
			&txs,
			// watching 2 addresses
			&map_script_addresses(vec![
				fake_details(deposit_address_1.clone()),
				fake_details(deposit_address_2.clone()),
			]),
		);

		// We should have one deposit per address.
		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses[0].amount, UTXO_FOR_SECOND_DEPOSIT);
		assert_eq!(deposit_witnesses[0].deposit_address, deposit_address_2.script_pubkey());
		assert_eq!(deposit_witnesses[1].amount, LARGEST_UTXO_TO_DEPOSIT);
		assert_eq!(deposit_witnesses[1].deposit_address, deposit_address_1.script_pubkey());
	}

	#[test]
	fn deposit_witnesses_ordering_is_consistent() {
		let address_1 = DepositAddress::new([0; 32], 9);
		let address_2 = DepositAddress::new([0; 32], 1232);
		DepositAddress::new([0; 32], 0);

		let addresses = map_script_addresses(vec![
			fake_details(address_1.clone()),
			fake_details(address_2.clone()),
		]);

		let txs: Vec<VerboseTransaction> = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(3, &address_1),
					(5, &DepositAddress::new([3; 32], 0)),
					(7, &address_1),
					(11, &address_2),
				]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![
					(13, &address_2),
					(17, &address_2),
					(19, &DepositAddress::new([5; 32], 0)),
					(23, &address_1),
				]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![
					(13, &address_2),
					(19, &DepositAddress::new([7; 32], 0)),
					(23, &address_1),
				]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![
					(29, &address_1),
					(31, &address_2),
					(37, &DepositAddress::new([11; 32], 0)),
				]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![(41, &address_2), (43, &DepositAddress::new([17; 32], 0))]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![(47, &address_1), (53, &DepositAddress::new([19; 32], 0))]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![(61, &DepositAddress::new([23; 32], 0)), (59, &address_2)]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![(67, &DepositAddress::new([29; 32], 0))]),
				None,
			),
		];

		let mut rng = rand::rngs::StdRng::from_seed([3; 32]);

		for _i in 0..10 {
			let n = rng.gen_range(0..txs.len());
			let test_txs = txs.as_slice().choose_multiple(&mut rng, n).cloned().collect::<Vec<_>>();
			assert!((0..10).map(|_| deposit_witnesses(&test_txs, &addresses)).all_equal());
		}
	}

	#[test]
	fn deposit_witnesses_several_diff_tx() {
		let address = DepositAddress::new([0; 32], 9);

		const UTXO_WITNESSED_1: u64 = 2324;
		const UTXO_WITNESSED_2: u64 = 1234;
		let txs = vec![
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_WITNESSED_1, &address),
					(12223, &DepositAddress::new([0; 32], 11)),
					(UTXO_WITNESSED_1 - 1, &address),
				]),
				None,
			),
			fake_transaction(
				fake_verbose_vouts(vec![
					(UTXO_WITNESSED_2 - 10, &address),
					(UTXO_WITNESSED_2, &address),
				]),
				None,
			),
		];

		let deposit_witnesses =
			deposit_witnesses(&txs, &map_script_addresses(vec![fake_details(address)]));
		assert_eq!(deposit_witnesses.len(), 2);
		assert_eq!(deposit_witnesses[0].amount, UTXO_WITNESSED_1);
		assert_eq!(deposit_witnesses[1].amount, UTXO_WITNESSED_2);
	}
}
