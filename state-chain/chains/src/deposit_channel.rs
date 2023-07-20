use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct DepositChannel<C: Chain> {
	// TODO: also add pending deposits into this as a Deque.
	pub channel_id: ChannelId,
	pub address: C::ChainAccount,
	pub asset: C::ChainAsset,
	pub state: C::DepositChannelState,
}

impl<C: Chain<DepositFetchId = ChannelId>> From<&DepositChannel<C>> for ChannelId {
	fn from(channel: &DepositChannel<C>) -> Self {
		channel.channel_id
	}
}

/// Defines the interface for chain-specific aspects of address management.
pub trait ChannelLifecycleHooks: Sized {
	/// Returns true if fetches can be made from the channel in the current state.
	fn can_fetch(&self) -> bool {
		true
	}

	/// Called when a fetch is scheduled for broadcast. Should return true if self was mutated.
	fn on_fetch_scheduled(&mut self) -> bool {
		false
	}

	/// Called when a fetch is completed. Should return true if self was mutated.
	fn on_fetch_completed(&mut self) -> bool {
		false
	}

	/// Returns Some(_) if the address can be re-used, otherwise None and the address is discarded.
	fn maybe_recycle(self) -> Option<Self> {
		None
	}
}

impl ChannelLifecycleHooks for () {}

impl<C: Chain> ChannelLifecycleHooks for DepositChannel<C> {
	fn can_fetch(&self) -> bool {
		self.state.can_fetch()
	}

	fn on_fetch_scheduled(&mut self) -> bool {
		self.state.on_fetch_scheduled()
	}

	fn on_fetch_completed(&mut self) -> bool {
		self.state.on_fetch_completed()
	}

	fn maybe_recycle(self) -> Option<Self> {
		self.state.maybe_recycle().map(|state| Self { state, ..self })
	}
}

impl<C: Chain> DepositChannel<C> {
	pub fn generate_new<A: AddressDerivationApi<C>>(
		channel_id: ChannelId,
		asset: C::ChainAsset,
	) -> Result<Self, DispatchError> {
		Ok(Self {
			channel_id,
			address: A::generate_address(asset, channel_id)?,
			asset,
			state: Default::default(),
		})
	}

	pub fn fetch_id(&self) -> C::DepositFetchId {
		self.into()
	}
}
