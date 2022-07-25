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
				log::trace!("No witness data injection for call {:?}", self);
			},
		}
	}
}
