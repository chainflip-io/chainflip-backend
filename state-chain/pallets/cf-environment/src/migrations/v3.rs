use crate::*;
use sp_std::{marker::PhantomData, vec};

pub struct Migration<T: Config>(PhantomData<T>);

pub mod types {
	use super::*;
	#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq)]
	pub struct Utxo {
		pub amount: u64,
		pub txid: [u8; 32],
		pub vout: u32,
		pub pubkey_x: [u8; 32],
		// Salt used to create the address that this utxo was sent to.
		pub salt: u32,
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		BitcoinAvailableUtxos::<T>::take();
		BitcoinAvailableUtxos::<T>::put(vec![Utxo {
			amount: 1_000_000,
			id: UtxoId {
				tx_id: hex_literal::hex!(
					"18f90ebe40abe55fcd940ab5a80f348c83775fb4ef93d99733b2cf2f4e8faddd"
				),
				vout: 1,
			},
			deposit_address: DepositAddress::new(
				hex_literal::hex!(
					"f937d2f21b80cb16357bed8e3d58463ba5bcc6fe0097d78ebf40aae9d5311612"
				),
				1,
			),
		}]);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, DispatchError> {
		let before = BitcoinAvailableUtxos::<T>::get();
		Ok(before.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), DispatchError> {
		assert_eq!(
			BitcoinAvailableUtxos::<T>::get(),
			vec![Utxo {
				amount: 1_000_000,
				id: UtxoId {
					tx_id: hex_literal::hex!(
						"18f90ebe40abe55fcd940ab5a80f348c83775fb4ef93d99733b2cf2f4e8faddd"
					),
					vout: 1,
				},
				deposit_address: DepositAddress::new(
					hex_literal::hex!(
						"f937d2f21b80cb16357bed8e3d58463ba5bcc6fe0097d78ebf40aae9d5311612"
					),
					1,
				),
			}],
			"UTXO not updated correctly"
		);

		Ok(())
	}
}
