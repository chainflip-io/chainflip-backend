use super::v3::types as v3_types;
use crate::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		BitcoinAvailableUtxos::<T>::translate::<Vec<v3_types::Utxo>, _>(|old_utxos_opt| {
			old_utxos_opt.map(|old_utxos| {
				old_utxos
					.into_iter()
					.map(|old| Utxo {
						amount: old.amount,
						id: UtxoId { tx_id: old.txid, vout: old.vout },
						deposit_address: DepositAddress::new(old.pubkey_x, old.salt),
					})
					.collect::<Vec<_>>()
			})
		})
		.unwrap_or_else(|_| {
			log::error!("Failed to migrate BitcoinAvailableUtxos");
			None
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, DispatchError> {
		let before = BitcoinAvailableUtxos::<T>::decode_len().unwrap_or(0) as u32;
		log::info!("before: {}", before);
		Ok(before.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), DispatchError> {
		log::info!("state: {:?}", state);
		let old_utxo_count =
			<u32>::decode(&mut &state[..]).map_err(|_| "Failed to decode old utxo count")?;
		assert_eq!(old_utxo_count, BitcoinAvailableUtxos::<T>::get().len() as u32);

		Ok(())
	}
}
