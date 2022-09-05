#[macro_export]
macro_rules! impl_mock_witnesser_for_account_and_call_types {
	($account_id:ty, $call:ty, $block_number:ty) => {
		pub struct MockWitnesser;

		impl MockWitnesser {
			pub fn set_threshold(threshold: u32) {
				WITNESS_THRESHOLD.with(|cell| *(cell.borrow_mut()) = threshold);
			}

			pub fn total_votes_cast() -> usize {
				WITNESS_VOTES.with(|cell| cell.borrow().len())
			}

			pub fn get_vote_count_for(call: &$call) -> usize {
				WITNESS_VOTES.with(|cell| cell.borrow().iter().filter(|c| *c == call).count())
			}
		}

		thread_local! {
			pub static WITNESS_THRESHOLD: std::cell::RefCell<u32> = std::cell::RefCell::new(0);
			pub static WITNESS_VOTES: std::cell::RefCell<Vec<$call>> = std::cell::RefCell::new(vec![]);
		}

		impl $crate::Witnesser for MockWitnesser {
			type AccountId = $account_id;
			type Call = $call;
			type BlockNumber = $block_number;

			fn witness_at_epoch(
				who: Self::AccountId,
				call: Self::Call,
				_epoch: cf_primitives::EpochIndex,
			) -> frame_support::dispatch::DispatchResultWithPostInfo {
				Self::witness(who, call)
			}

			fn witness(
				_who: Self::AccountId,
				call: Self::Call,
			) -> frame_support::dispatch::DispatchResultWithPostInfo {
				let count = WITNESS_VOTES.with(|votes| {
					let mut votes = votes.borrow_mut();
					votes.push(call.clone());
					votes.iter().filter(|vote| **vote == call.clone()).count()
				});

				let threshold = WITNESS_THRESHOLD.with(|t| t.borrow().clone());

				if count as u32 == threshold {
					frame_support::dispatch::Dispatchable::dispatch(call, Origin::root())
				} else {
					Ok(().into())
				}
			}
		}
	};
}
