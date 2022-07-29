use crate::{Call, EthereumInstance, Runtime};
use codec::{Decode, Encode};
use pallet_cf_witnesser::WitnessDataExtraction;
use sp_std::{mem, prelude::*};

impl WitnessDataExtraction for Call {
	fn extract(&mut self) -> Option<Vec<u8>> {
		match self {
			Call::EthereumChainTracking(pallet_cf_chain_tracking::Call::<
				Runtime,
				EthereumInstance,
			>::update_chain_state {
				ref mut state,
			}) => {
				let priority_fee = mem::take(&mut state.priority_fee);
				Some(priority_fee.encode())
			},
			_ => None,
		}
	}

	fn combine_and_inject(&mut self, data: &mut [Vec<u8>]) {
		if data.is_empty() {
			return
		}

		match self {
			Call::EthereumChainTracking(pallet_cf_chain_tracking::Call::<
				Runtime,
				EthereumInstance,
			>::update_chain_state {
				state,
			}) => {
				// Encoding is order-preserving so we can sort the raw encoded bytes and then decode
				// just the result.
				let len = data.len();
				let median_index = if len % 2 == 0 { (len - 1) / 2 } else { len / 2 };
				let (_, median_bytes, _) = data.select_nth_unstable(median_index);

				match Decode::decode(&mut &median_bytes[..]) {
					Ok(median) => {
						state.priority_fee = median;
					},
					Err(e) => {
						log::error!("Failed to decode median priority fee: {:?}", e);
					},
				}
			},
			_ => {
				log::warn!("No witness data injection for call {:?}", self);
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Origin, Validator, Witnesser};
	use cf_chains::{eth::TrackedData, Chain, Ethereum};
	use cf_traits::EpochInfo;
	use frame_support::{assert_ok, Hashable};
	use pallet_cf_witnesser::CallHash;
	use sp_std::iter;

	fn eth_chain_tracking_call_with_fee(priority_fee: <Ethereum as Chain>::ChainAmount) -> Call {
		Call::EthereumChainTracking(
			pallet_cf_chain_tracking::Call::<Runtime, EthereumInstance>::update_chain_state {
				state: TrackedData { block_height: 1_000, base_fee: 40, priority_fee },
			},
		)
	}

	#[test]
	fn test_medians() {
		test_priority_fee_median([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 5);
		test_priority_fee_median([1, 2, 3, 4, 6, 6, 7, 8, 9, 10], 6);
		test_priority_fee_median([0, 0, 1, 1, 2, 3, 3, 4, 6], 2);
	}

	fn test_priority_fee_median<const S: usize>(fees: [u128; S], expected_median: u128) {
		let mut calls = fees.map(eth_chain_tracking_call_with_fee);

		let call_hashes = calls.iter().map(|call| CallHash(call.blake2_256())).collect::<Vec<_>>();
		assert!(
			!iter::zip(call_hashes.iter(), call_hashes.iter().skip(1)).all(|(a, b)| a == b),
			"Call hashes should be different before extraction."
		);

		let mut extracted_data =
			calls.iter_mut().map(|call| call.extract().unwrap()).collect::<Vec<_>>();

		let call_hashes = calls.iter().map(|call| CallHash(call.blake2_256())).collect::<Vec<_>>();
		assert!(
			iter::zip(call_hashes.iter(), call_hashes.iter().skip(1)).all(|(a, b)| a == b),
			"Call hashes should all be equal after extraction."
		);

		let mut threshold_call = calls.last().unwrap().clone();
		threshold_call.combine_and_inject(&mut extracted_data[..]);

		assert_eq!(threshold_call, eth_chain_tracking_call_with_fee(expected_median));
	}

	#[test]
	fn test_priority_fee_witnessing() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			assert!(
				pallet_cf_chain_tracking::ChainState::<Runtime, EthereumInstance>::get().is_none()
			);

			let calls = [1, 100, 10, 10, 10, 10].map(eth_chain_tracking_call_with_fee);

			let authorities = (0..calls.len()).map(|i| [i as u8; 32].into()).collect::<Vec<_>>();
			pallet_cf_validator::CurrentEpoch::<Runtime>::put(1);
			pallet_cf_validator::CurrentAuthorities::<Runtime>::put(&authorities);
			pallet_cf_validator::EpochAuthorityCount::<Runtime>::insert(
				<Validator as EpochInfo>::epoch_index(),
				authorities.len() as u32,
			);

			for (index, authority_id) in authorities.into_iter().enumerate() {
				pallet_cf_validator::AuthorityIndex::<Runtime>::insert(
					<Validator as EpochInfo>::epoch_index(),
					&authority_id,
					index as u32,
				);
				assert_ok!(Witnesser::witness(
					Origin::signed(authority_id),
					Box::new(calls[index].clone())
				));
			}

			assert_eq!(
				pallet_cf_chain_tracking::ChainState::<Runtime, EthereumInstance>::get().unwrap(),
				TrackedData { block_height: 1_000, base_fee: 40, priority_fee: 10 }
			);
		})
	}
}
