// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use codec::{Decode, Encode};
use sp_std::{collections::btree_set::BTreeSet, fmt::Debug, marker::PhantomData};

use crate::offence_reporting::OffenceReporter;

use super::{MockPallet, MockPalletStorage};

pub struct MockOffenceReporter<V, O>(PhantomData<(V, O)>);

impl<ValidatorId, Offence> MockOffenceReporter<ValidatorId, Offence>
where
	ValidatorId: Encode + Decode + Debug + Copy + Ord,
	Offence: Encode + Decode + Copy,
{
	fn mock_report_many(offence: Offence, validators: impl IntoIterator<Item = ValidatorId>) {
		let mut reported = Self::get_reported_for(offence);
		validators.into_iter().for_each(|id| {
			reported.insert(id);
		});
		Self::set_reported_for(offence, reported);
	}

	fn get_reported_for(offence: Offence) -> BTreeSet<ValidatorId> {
		Self::get_storage(b"Reported", offence.encode()).unwrap_or_default()
	}

	fn set_reported_for(offence: Offence, validators: impl IntoIterator<Item = ValidatorId>) {
		Self::put_storage(b"Reported", offence.encode(), BTreeSet::from_iter(validators))
	}

	pub fn assert_reported(offence: Offence, validators: impl IntoIterator<Item = ValidatorId>) {
		assert_eq!(Self::get_reported_for(offence), BTreeSet::from_iter(validators),)
	}
}

impl<T, O> MockPallet for MockOffenceReporter<T, O> {
	const PREFIX: &'static [u8] = b"MockOffenceReporter";
}

impl<ValidatorId: 'static, Offence> OffenceReporter for MockOffenceReporter<ValidatorId, Offence>
where
	ValidatorId: Encode + Decode + Debug + Copy + Ord,
	Offence: Encode + Decode + Copy,
{
	type ValidatorId = ValidatorId;
	type Offence = Offence;

	fn report_many(
		offence: impl Into<Self::Offence>,
		validators: impl IntoIterator<Item = ValidatorId>,
	) {
		Self::mock_report_many(offence.into(), validators);
	}

	fn forgive_all(offence: impl Into<Self::Offence>) {
		Self::set_reported_for(offence.into(), []);
	}
}

#[cfg(test)]
mod test {
	use scale_info::TypeInfo;

	use super::*;

	#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
	enum MockOffence {
		BeingNaughty,
		BeingSuperNaughty,
	}

	type TestOffenceReporter = MockOffenceReporter<u64, MockOffence>;

	#[test]
	fn test_offence_reporter_mock() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			TestOffenceReporter::report(MockOffence::BeingNaughty, 1);
			TestOffenceReporter::report_many(MockOffence::BeingNaughty, [1, 2, 3]);
			TestOffenceReporter::report_many(MockOffence::BeingSuperNaughty, [2, 3, 4]);

			assert_eq!(
				TestOffenceReporter::get_reported_for(MockOffence::BeingNaughty),
				BTreeSet::from_iter([1, 2, 3])
			);
			assert_eq!(
				TestOffenceReporter::get_reported_for(MockOffence::BeingSuperNaughty),
				BTreeSet::from_iter([2, 3, 4])
			);
		});
	}
}
