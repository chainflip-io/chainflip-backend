use crate::*;
use sp_std::{marker::PhantomData, vec};

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		BitcoinAvailableUtxos::<T>::take();
		BitcoinAvailableUtxos::<T>::put(vec![Utxo {
			amount: 1_000_000,
			txid: hex_literal::hex!(
				"18f90ebe40abe55fcd940ab5a80f348c83775fb4ef93d99733b2cf2f4e8faddd"
			),
			vout: 1,
			pubkey_x: hex_literal::hex!(
				"f937d2f21b80cb16357bed8e3d58463ba5bcc6fe0097d78ebf40aae9d5311612"
			),
			salt: 1,
		}]);
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, &'static str> {
		let before = BitcoinAvailableUtxos::<T>::get();
		Ok(before.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), &'static str> {
		assert_eq!(
			BitcoinAvailableUtxos::<T>::get(),
			vec![Utxo {
				amount: 1_000_000,
				txid: hex_literal::hex!(
					"18f90ebe40abe55fcd940ab5a80f348c83775fb4ef93d99733b2cf2f4e8faddd"
				),
				vout: 1,
				pubkey_x: hex_literal::hex!(
					"f937d2f21b80cb16357bed8e3d58463ba5bcc6fe0097d78ebf40aae9d5311612"
				),
				salt: 1,
			}],
			"UTXO not updated correctly"
		);

		Ok(())
	}
}
