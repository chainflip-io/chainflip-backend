use crate::*;
use cf_chains::{ChainCrypto, Ethereum};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::{collections::btree_set::BTreeSet, marker::PhantomData};

pub struct Migration<T: Config>(PhantomData<T>);

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
		let awaiting = GovKeyUpdateAwaitingEnactment::<T>::get().is_some();
		let proposal_count = Proposals::<T>::iter_keys().count() as u32;
		Ok((awaiting, proposal_count).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
		let (awaiting, proposal_count) = <(bool, u32)>::decode(&mut &state[..]).unwrap();
		ensure!(
			GovKeyUpdateAwaitingEnactment::<T>::get().is_some() == awaiting,
			"GovKeyUpdateAwaitingEnactment migration failed."
		);
		ensure!(
			Proposals::<T>::iter_keys().count() as u32 == proposal_count,
			"Proposals migration failed."
		);
		ensure!(old::Backers::<T>::drain().count() == 0, "Old storage for Backers not cleared.");
		ensure!(
			old::Proposals::<T>::drain().count() == 0,
			"Old storage for Proposals not cleared."
		);
		Ok(())
	}
}

#[cfg(test)]
mod test_runtime_upgrade {
	use super::*;
	use mock::Test;

	pub const GOV_KEY: [u8; 20] = [0xcf; 20];
	pub const BACKERS: &[u64; 3] = &[1, 2, 3];
	pub const CHAIN: ForeignChain = ForeignChain::Ethereum;

	#[test]
	fn test() {
		mock::new_test_ext().execute_with(|| {
			// pre upgrade
			let proposal = ProposalV0::SetGovernanceKey(GOV_KEY.into());
			let block = <frame_system::Pallet<Test>>::block_number() +
				<Test as Config>::VotingPeriod::get();
			old::Proposals::<Test>::insert(block, proposal.clone());
			old::Backers::<Test>::insert(proposal, BACKERS.as_slice());

			// upgrade
			Migration::<Test>::on_runtime_upgrade();

			// post upgrade
			let expected_proposal = Proposal::SetGovernanceKey(CHAIN, GOV_KEY.into());
			assert_eq!(
				Proposals::<Test>::get(block).unwrap(),
				expected_proposal,
				"Proposal translation is incorrect."
			);
			assert_eq!(
				Backers::<Test>::get(&expected_proposal),
				BTreeSet::<_>::from_iter(BACKERS.clone()),
				"Backers accounts are incorrect."
			);
		})
	}
}
