use super::*;
use crate::{Chain, ChainflipEnvironment, DepositChannel};
use cf_primitives::{chains::Polkadot, ChannelId};
use cf_utilities::SliceToArray;
use codec::MaxEncodedLen;
use frame_support::sp_runtime::{traits::BlakeTwo256, DispatchError};
use sp_core::Get;
use sp_std::mem::size_of;

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct PolkadotDepositChannel {
	channel_id: ChannelId,
	address: PolkadotAccountId,
}

impl DepositChannel<Polkadot> for PolkadotDepositChannel {
	type Deposit = ();

	fn generate_new<E: ChainflipEnvironment>(
		channel_id: cf_primitives::ChannelId,
		_asset: <Polkadot as Chain>::ChainAsset,
	) -> Result<Self, DispatchError> {
		const PREFIX: &[u8; 16] = b"modlpy/utilisuba";
		const RAW_PUBLIC_KEY_SIZE: usize = 32;
		const PAYLOAD_LENGTH: usize = PREFIX.len() + RAW_PUBLIC_KEY_SIZE + size_of::<u16>();

		let mut layers = channel_id
			.to_be_bytes()
			.chunks(2)
			.map(|chunk| u16::from_be_bytes(chunk.as_array::<2>()))
			.skip_while(|layer| *layer == 0u16)
			.collect::<Vec<u16>>();

		layers.reverse();

		let payload_hash = layers.into_iter().fold(
			*<E as Get<PolkadotAccountId>>::get().aliased_ref(),
			|sub_account, salt| {
				let mut payload = Vec::with_capacity(PAYLOAD_LENGTH);
				// Fill the first slots with the derivation prefix.
				payload.extend(PREFIX);
				// Then add the 32-byte public key.
				payload.extend(sub_account);
				// Finally, add the index to the end of the payload.
				payload.extend(&salt.to_le_bytes());

				// Hash the whole thing
				BlakeTwo256::hash(&payload).to_fixed_bytes()
			},
		);

		Ok(Self { channel_id, address: PolkadotAccountId::from_aliased(payload_hash) })
	}

	fn channel_id(&self) -> cf_primitives::ChannelId {
		self.channel_id
	}

	fn address(&self) -> &<Polkadot as Chain>::ChainAccount {
		&self.address
	}

	fn fetch_params(&self, _: Self::Deposit) -> <Polkadot as Chain>::FetchParams {
		self.channel_id
	}
}
