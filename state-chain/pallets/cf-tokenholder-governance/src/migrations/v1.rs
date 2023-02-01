use crate::*;
use cf_chains::{ChainCrypto, Ethereum};
use cf_primitives::BlockNumber;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

pub struct Migration<T: Config>(PhantomData<T>);

#[cfg(feature = "try-runtime")]
mod try_runtime_tests {
	use super::*;

	pub const GOV_KEY: [u8; 20] = [0xcf; 20];
	pub const SUBMITTER: [u8; 32] = [0xcf; 32];
	pub const CHAIN: ForeignChain = ForeignChain::Ethereum;

	pub fn submitter_account_id<T: frame_system::Config>() -> T::AccountId {
		Decode::decode(&mut &SUBMITTER[..]).unwrap()
	}
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum ProposalV0 {
	SetGovernanceKey(<Ethereum as ChainCrypto>::GovKey),
	SetCommunityKey(<Ethereum as ChainCrypto>::GovKey),
}

mod old {
	use super::*;
	use frame_support::Twox64Concat;

	#[frame_support::storage_alias]
	pub type Proposals<T: Config> =
		StorageMap<TokenholderGovernance, Twox64Concat, BlockNumberFor<T>, ProposalV0>;

	#[frame_support::storage_alias]
	pub type Backers<T: Config> = StorageMap<
		TokenholderGovernance,
		Twox64Concat,
		ProposalV0,
		Vec<<T as frame_system::Config>::AccountId>,
	>;
}

impl From<ProposalV0> for Proposal {
	fn from(old: ProposalV0) -> Self {
		match old {
			ProposalV0::SetGovernanceKey(ref new_key) =>
				Proposal::SetGovernanceKey(ForeignChain::Ethereum, new_key.encode()),
			ProposalV0::SetCommunityKey(new_key) => Proposal::SetCommunityKey(new_key),
		}
	}
}

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Proposals::<T>::translate_values::<ProposalV0, _>(|proposal_v0| Some(proposal_v0.into()));
		// Collect this into a vec first to avoid mutating the map in-place.
		for (proposal_v1, backers_set) in old::Backers::<T>::drain()
			.map(|(proposal_v0, backers_vec)| {
				(proposal_v0.into(), BTreeSet::<_>::from_iter(backers_vec))
			})
			.collect::<Vec<(Proposal, BTreeSet<_>)>>()
		{
			Backers::<T>::insert(proposal_v1, backers_set);
		}
		GovKeyUpdateAwaitingEnactment::<T>::translate::<
			(BlockNumberFor<T>, <Ethereum as ChainCrypto>::GovKey),
			_,
		>(|maybe_update| {
			maybe_update.map(|(block_number, eth_gov_key)| {
				(block_number, (ForeignChain::Ethereum, eth_gov_key.encode()))
			})
		})
		.expect("Decoding of old type shouldn't fail");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
		// Ensure we have a proposal to migrate in case there are none on the test chain.
		let proposal = ProposalV0::SetGovernanceKey(try_runtime_tests::GOV_KEY.into());
		let block = <frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get();
		old::Proposals::<T>::insert(
			<frame_system::Pallet<T>>::block_number() + T::VotingPeriod::get(),
			proposal.clone(),
		);
		old::Backers::<T>::insert(proposal, vec![try_runtime_tests::submitter_account_id::<T>()]);
		Ok(block.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(block_encoded: Vec<u8>) -> Result<(), &'static str> {
		let block = <BlockNumberFor<T> as Decode>::decode(&mut &block_encoded[..]).unwrap();
		let expected_proposal =
			Proposal::SetGovernanceKey(try_runtime_tests::CHAIN, try_runtime_tests::GOV_KEY.into());
		ensure!(
			Proposals::<T>::get(block)
				.expect("Proposal should have been inserted during pre upgrade") ==
				expected_proposal,
			"Proposal translation is incorrect."
		);
		ensure!(
			Backers::<T>::get(&expected_proposal)
				.contains(&try_runtime_tests::submitter_account_id::<T>()),
			"Submitter account is missing."
		);
		Ok(())
	}
}
