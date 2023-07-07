use crate::{Environment, EthEnvironment};
use cf_chains::{
	eth::{api::EthEnvironmentProvider, deposit_address::get_create_2_address, EthereumChannelId},
	Chain, Ethereum,
};
use cf_primitives::{chains::assets::eth, ChannelId};
use cf_traits::{AddressDerivationApi, DepositChannel};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::H160;
use sp_runtime::DispatchError;

use super::AddressDerivation;

impl AddressDerivationApi<Ethereum> for AddressDerivation {
	fn generate_address(
		source_asset: eth::Asset,
		channel_id: ChannelId,
	) -> Result<<Ethereum as Chain>::ChainAccount, DispatchError> {
		Ok(get_create_2_address(
			Environment::eth_vault_address(),
			EthEnvironment::token_address(source_asset).map(|address| address.to_fixed_bytes()),
			channel_id,
		)
		.into())
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
pub enum DeploymentStatus {
	Deployed,
	Pending,
	Undeployed,
}

#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Copy, Debug)]
pub struct EthereumDepositAddress {
	pub address: H160,
	pub channel_id: u64,
	pub asset: eth::Asset,
	pub deployment_status: DeploymentStatus,
	pub deposit_fetch_id: EthereumChannelId,
}

impl DepositChannel<Ethereum> for EthereumDepositAddress {
	type Address = H160;
	type DepositFetchId = EthereumChannelId;
	type AddressDerivation = AddressDerivation;

	fn get_address(&self) -> Self::Address {
		self.address
	}

	fn process_broadcast(mut self) -> (Self, bool)
	where
		Self: Sized,
	{
		match self.deployment_status {
			DeploymentStatus::Deployed => (self, true),
			DeploymentStatus::Pending => (self, false),
			DeploymentStatus::Undeployed => {
				self.deployment_status = DeploymentStatus::Pending;
				(self, true)
			},
		}
	}

	fn get_deposit_fetch_id(&self) -> Self::DepositFetchId {
		self.deposit_fetch_id
	}

	fn new(channel_id: u64, asset: <Ethereum as Chain>::ChainAsset) -> Result<Self, DispatchError>
	where
		Self: Sized,
	{
		let address = <AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			asset, channel_id,
		)?;
		Ok(Self {
			address,
			channel_id,
			deployment_status: DeploymentStatus::Undeployed,
			deposit_fetch_id: EthereumChannelId::UnDeployed(channel_id),
			asset,
		})
	}

	fn maybe_recycle(&self) -> bool
	where
		Self: Sized,
	{
		self.deployment_status == DeploymentStatus::Deployed
	}

	fn finalize(mut self) -> Self
	where
		Self: Sized,
	{
		match self.deployment_status {
			DeploymentStatus::Pending => {
				self.deposit_fetch_id = EthereumChannelId::Deployed(self.address);
				self.deployment_status = DeploymentStatus::Deployed;
			},
			DeploymentStatus::Undeployed => self.deployment_status = DeploymentStatus::Pending,
			_ => (),
		}
		self
	}

	fn get_channel_id(&self) -> u64 {
		self.channel_id
	}

	fn get_asset(&self) -> <Ethereum as Chain>::ChainAsset {
		self.asset
	}
}

#[test]
fn test_address_generation() {
	use crate::Runtime;
	use cf_chains::Ethereum;
	use cf_primitives::chains::assets::eth::Asset;
	use pallet_cf_environment::EthereumSupportedAssets;

	frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
		// Expect address generation to be successfully for native ETH
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Eth,
			1
		)
		.is_ok());
		// The genesis build is not running, so we have to add it manually
		EthereumSupportedAssets::<Runtime>::insert(Asset::Flip, [0; 20]);
		// Expect address generation to be successfully for ERC20 Flip token
		assert!(<AddressDerivation as AddressDerivationApi<Ethereum>>::generate_address(
			eth::Asset::Flip,
			1
		)
		.is_ok());

		// Address derivation for Dot is currently unimplemented.
		// Expect address generation to return an error for unsupported assets. Because we are
		// running a test gainst ETH the DOT asset will be always unsupported.
		// assert!(AddressDerivation::generate_address(Asset::Dot, 1).is_err());
	});
}
