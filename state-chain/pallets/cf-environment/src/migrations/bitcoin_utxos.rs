use crate::*;
use cf_chains::btc::deposit_address::TapscriptPath;
use frame_support::traits::OnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;
	use cf_chains::btc::{BitcoinScript, Hash};

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct DepositAddress {
		pub pubkey_x: [u8; 32],
		pub salt: u32,
		pub tweaked_pubkey_bytes: [u8; 33],
		pub tapleaf_hash: Hash,
		pub unlock_script: BitcoinScript,
	}

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub struct Utxo {
		pub id: UtxoId,
		pub amount: u64,
		pub deposit_address: old::DepositAddress,
	}

	#[frame_support::storage_alias]
	pub type BitcoinAvailableUtxos<T: Config> = StorageValue<Pallet<T>, Vec<old::Utxo>, ValueQuery>;
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let old_utxos = old::BitcoinAvailableUtxos::<T>::take();
		let new_utxos: Vec<Utxo> = old_utxos
			.iter()
			.map(|utxo| Utxo {
				id: utxo.id.clone(),
				amount: utxo.amount,
				deposit_address: DepositAddress {
					pubkey_x: utxo.deposit_address.pubkey_x,
					script_path: Some(TapscriptPath {
						salt: utxo.deposit_address.salt,
						tweaked_pubkey_bytes: utxo.deposit_address.tweaked_pubkey_bytes,
						tapleaf_hash: utxo.deposit_address.tapleaf_hash,
						unlock_script: utxo.deposit_address.unlock_script.clone(),
					}),
				},
			})
			.collect();

		BitcoinAvailableUtxos::<T>::put(new_utxos);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
