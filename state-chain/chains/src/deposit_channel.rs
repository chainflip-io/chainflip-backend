use super::*;

pub trait DepositChannel<C: Chain>
where
	Self: Sized,
{
	type Deposit: Member + Parameter;

	/// Generates a new deposit channel.
	fn generate_new<E: ChainflipEnvironment>(
		channel_id: ChannelId,
		asset: C::ChainAsset,
	) -> Result<Self, DispatchError>;

	/// The channel id that was used to generate this deposit channel.
	fn channel_id(&self) -> ChannelId;

	/// If the channel can only be used for a specific asset, returns that asset, otherwise returns
	/// None to signify that the channel is asset-agnostic.
	fn asset(&self) -> Option<C::ChainAsset> {
		None
	}

	/// Returns the address associated with the deposit channel.
	fn address(&self) -> &C::ChainAccount;

	/// Returns the chain-specific fetch parameters for a specific deposit.
	fn fetch_params(&self, deposit: Self::Deposit) -> C::FetchParams;
}

impl<C: Chain> DepositChannel<C> for () {
	type Deposit = ();

	fn generate_new<E: ChainflipEnvironment>(
		_channel_id: ChannelId,
		_asset: <C as Chain>::ChainAsset,
	) -> Result<Self, AddressDerivationError> {
		Err(DispatchError::Other("Unimplemented."))
	}

	fn channel_id(&self) -> ChannelId {
		unimplemented!()
	}

	fn address(&self) -> &<C as Chain>::ChainAccount {
		unimplemented!()
	}

	fn fetch_params(&self, _deposit: Self::Deposit) -> <C as Chain>::FetchParams {
		unimplemented!()
	}
}
