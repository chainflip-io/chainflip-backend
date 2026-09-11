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

use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{ensure, pallet_prelude::RuntimeDebug};
use scale_info::TypeInfo;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

pub type VoteWeight = u32;

/// Counts an `Individual` as depth 1, so this allows root group -> sub-group -> individual.
pub const MAX_DEPTH: u32 = 3;
/// Bounds the cost of re-evaluating the quorum on every approval.
pub const MAX_MEMBERS: usize = 64;

/// A (possibly nested) body whose members' approvals decide whether a proposal passes.
///
/// Approvals are tracked as a flat set of individual accounts: a group reaches quorum once enough
/// of its members do, counted by number (`SimpleGroup`) or by weight (`WeightedGroup`).
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
#[codec(encode_bound(AccountId: Encode))]
#[codec(decode_bound(AccountId: Decode))]
#[codec(decode_with_mem_tracking_bound(AccountId: DecodeWithMemTracking))]
#[scale_info(bounds(AccountId: TypeInfo + 'static))]
pub enum Council<AccountId> {
	WeightedGroup { threshold: VoteWeight, members: Vec<(VoteWeight, Council<AccountId>)> },
	SimpleGroup { threshold: u8, members: Vec<Council<AccountId>> },
	Individual { id: AccountId },
}

/// Unreachable by construction: nobody is a member, so nothing can ever be approved.
impl<AccountId> Default for Council<AccountId> {
	fn default() -> Self {
		Self::SimpleGroup { threshold: 1, members: Vec::new() }
	}
}

#[derive(Clone, Copy, RuntimeDebug, PartialEq, Eq)]
pub enum InvalidCouncil {
	TooDeep,
	TooManyMembers,
	EmptyGroup,
	ZeroThreshold,
	ZeroWeight,
	WeightOverflow,
	ThresholdUnreachable,
	DuplicateMember,
}

impl<AccountId: Ord + Clone> Council<AccountId> {
	pub fn simple_group(threshold: u8, members: impl IntoIterator<Item = AccountId>) -> Self {
		Self::SimpleGroup {
			threshold,
			members: members.into_iter().map(|id| Self::Individual { id }).collect(),
		}
	}

	pub fn quorum_reached(&self, approvals: &BTreeSet<AccountId>) -> bool {
		match self {
			Self::Individual { id } => approvals.contains(id),
			Self::SimpleGroup { threshold, members } =>
				members.iter().filter(|member| member.quorum_reached(approvals)).count() >=
					*threshold as usize,
			Self::WeightedGroup { threshold, members } =>
				members
					.iter()
					.filter(|(_, member)| member.quorum_reached(approvals))
					.fold(0, |total: VoteWeight, (weight, _)| total.saturating_add(*weight)) >=
					*threshold,
		}
	}

	pub fn is_member(&self, who: &AccountId) -> bool {
		match self {
			Self::Individual { id } => id == who,
			Self::SimpleGroup { members, .. } => members.iter().any(|member| member.is_member(who)),
			Self::WeightedGroup { members, .. } =>
				members.iter().any(|(_, member)| member.is_member(who)),
		}
	}

	pub fn members(&self) -> BTreeSet<AccountId> {
		match self {
			Self::Individual { id } => BTreeSet::from([id.clone()]),
			Self::SimpleGroup { members, .. } => members.iter().flat_map(Self::members).collect(),
			Self::WeightedGroup { members, .. } =>
				members.iter().flat_map(|(_, member)| member.members()).collect(),
		}
	}

	/// Guarantees the council can always reach quorum (all members approving) and can never
	/// reach it without approvals.
	pub fn validate(&self) -> Result<(), InvalidCouncil> {
		self.validate_at(1, &mut BTreeSet::new())
	}

	fn validate_at<'a>(
		&'a self,
		depth: u32,
		seen: &mut BTreeSet<&'a AccountId>,
	) -> Result<(), InvalidCouncil> {
		ensure!(depth <= MAX_DEPTH, InvalidCouncil::TooDeep);
		match self {
			Self::Individual { id } => {
				ensure!(seen.insert(id), InvalidCouncil::DuplicateMember);
				ensure!(seen.len() <= MAX_MEMBERS, InvalidCouncil::TooManyMembers);
			},
			Self::SimpleGroup { threshold, members } => {
				ensure!(!members.is_empty(), InvalidCouncil::EmptyGroup);
				ensure!(*threshold > 0, InvalidCouncil::ZeroThreshold);
				ensure!(*threshold as usize <= members.len(), InvalidCouncil::ThresholdUnreachable);
				for member in members {
					member.validate_at(depth.saturating_add(1), seen)?;
				}
			},
			Self::WeightedGroup { threshold, members } => {
				ensure!(!members.is_empty(), InvalidCouncil::EmptyGroup);
				ensure!(*threshold > 0, InvalidCouncil::ZeroThreshold);
				let mut total: VoteWeight = 0;
				for (weight, member) in members {
					ensure!(*weight > 0, InvalidCouncil::ZeroWeight);
					total = total.checked_add(*weight).ok_or(InvalidCouncil::WeightOverflow)?;
					member.validate_at(depth.saturating_add(1), seen)?;
				}
				ensure!(*threshold <= total, InvalidCouncil::ThresholdUnreachable);
			},
		}
		Ok(())
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::{
		Council::{Individual, SimpleGroup, WeightedGroup},
		InvalidCouncil::*,
		*,
	};
	use proptest::{prelude::*, proptest};

	pub fn pro_3132_council() -> Council<u64> {
		WeightedGroup {
			threshold: 50,
			members: vec![
				(30, Council::simple_group(3, 101..=107)),
				(20, Council::simple_group(2, 201..=204)),
				(20, Individual { id: 301 }),
				(15, Individual { id: 302 }),
				(15, Individual { id: 303 }),
			],
		}
	}

	fn approvals(ids: impl IntoIterator<Item = u64>) -> BTreeSet<u64> {
		ids.into_iter().collect()
	}

	#[test]
	fn simple_group_counts_members() {
		let council = Council::simple_group(2, [1, 2, 3]);
		assert!(!council.quorum_reached(&approvals([1])));
		assert!(council.quorum_reached(&approvals([1, 3])));
	}

	#[test]
	fn pro_3132_scenario() {
		let council = pro_3132_council();
		assert_eq!(council.validate(), Ok(()));
		// Member 1 of group 1 proposes; individual 1 approves: 20 of 50.
		assert!(!council.quorum_reached(&approvals([101, 301])));
		// Group 1 is still short of 3-of-7.
		assert!(!council.quorum_reached(&approvals([101, 102, 301])));
		// Group 1 passes internally: 30 + 20 reaches 50.
		assert!(council.quorum_reached(&approvals([101, 102, 103, 301])));
		// 30 + 15 does not.
		assert!(!council.quorum_reached(&approvals([101, 102, 103, 302])));
	}

	#[test]
	fn membership_is_flattened() {
		let council = pro_3132_council();
		assert!(council.is_member(&104));
		assert!(council.is_member(&303));
		assert!(!council.is_member(&108));
		assert_eq!(council.members().len(), 7 + 4 + 3);
	}

	#[test]
	fn validation_rejects_invalid_councils() {
		let deep = SimpleGroup {
			threshold: 1,
			members: vec![SimpleGroup {
				threshold: 1,
				members: vec![Council::simple_group(1, [1])],
			}],
		};
		for (council, error) in [
			(deep, TooDeep),
			(Council::simple_group(1, 0..=MAX_MEMBERS as u64), TooManyMembers),
			(SimpleGroup { threshold: 1, members: vec![] }, EmptyGroup),
			(WeightedGroup { threshold: 1, members: vec![] }, EmptyGroup),
			(Council::simple_group(0, [1]), ZeroThreshold),
			(
				WeightedGroup { threshold: 0, members: vec![(1, Individual { id: 1 })] },
				ZeroThreshold,
			),
			(Council::simple_group(3, [1, 2]), ThresholdUnreachable),
			(
				WeightedGroup {
					threshold: 11,
					members: vec![(5, Individual { id: 1 }), (5, Individual { id: 2 })],
				},
				ThresholdUnreachable,
			),
			(
				WeightedGroup {
					threshold: 1,
					members: vec![(0, Individual { id: 1 }), (1, Individual { id: 2 })],
				},
				ZeroWeight,
			),
			(
				WeightedGroup {
					threshold: 1,
					members: vec![(u32::MAX, Individual { id: 1 }), (1, Individual { id: 2 })],
				},
				WeightOverflow,
			),
			(
				SimpleGroup {
					threshold: 1,
					members: vec![Individual { id: 1 }, Council::simple_group(1, [1])],
				},
				DuplicateMember,
			),
		] {
			assert_eq!(council.validate(), Err(error), "{council:?}");
		}
	}

	#[test]
	fn validation_accepts_edge_cases() {
		assert_eq!(Individual { id: 1u64 }.validate(), Ok(()));
		assert_eq!(Council::simple_group(1, 0..MAX_MEMBERS as u64).validate(), Ok(()));
		assert_eq!(
			Council::simple_group(MAX_MEMBERS as u8, 0..MAX_MEMBERS as u64).validate(),
			Ok(())
		);
	}

	#[test]
	fn default_is_unreachable() {
		let council = Council::<u64>::default();
		assert!(!council.quorum_reached(&approvals([1, 2, 3])));
		assert!(council.members().is_empty());
	}

	fn depth(council: &Council<u64>) -> u32 {
		match council {
			Individual { .. } => 1,
			SimpleGroup { members, .. } => 1 + members.iter().map(depth).max().unwrap_or(0),
			WeightedGroup { members, .. } =>
				1 + members.iter().map(|(_, member)| depth(member)).max().unwrap_or(0),
		}
	}

	fn leaves(council: &Council<u64>) -> Vec<u64> {
		match council {
			Individual { id } => vec![*id],
			SimpleGroup { members, .. } => members.iter().flat_map(leaves).collect(),
			WeightedGroup { members, .. } =>
				members.iter().flat_map(|(_, member)| leaves(member)).collect(),
		}
	}

	/// Arbitrary trees, mostly invalid: small id space (duplicates), zero/huge weights and
	/// thresholds, empty groups, and one level deeper than `MAX_DEPTH` allows.
	fn arb_council() -> impl Strategy<Value = Council<u64>> {
		(0u64..24).prop_map(|id| Individual { id }).prop_recursive(3, 64, 6, |inner| {
			prop_oneof![
				(0u8..8, prop::collection::vec(inner.clone(), 0..7))
					.prop_map(|(threshold, members)| SimpleGroup { threshold, members }),
				(
					prop_oneof![0u32..40, Just(u32::MAX)],
					prop::collection::vec((prop_oneof![0u32..10, Just(u32::MAX)], inner), 0..7),
				)
					.prop_map(|(threshold, members)| WeightedGroup { threshold, members }),
			]
		})
	}

	/// Makes an arbitrary tree of depth <= `MAX_DEPTH` valid: unique ids, non-zero weights,
	/// thresholds clamped into `1..=max`, and empty groups replaced by a fresh individual.
	fn normalise(council: Council<u64>, next_id: &mut u64) -> Council<u64> {
		match council {
			SimpleGroup { threshold, members } if !members.is_empty() => {
				let members: Vec<_> =
					members.into_iter().map(|member| normalise(member, next_id)).collect();
				SimpleGroup { threshold: 1 + threshold % members.len() as u8, members }
			},
			WeightedGroup { threshold, members } if !members.is_empty() => {
				let members: Vec<_> = members
					.into_iter()
					.map(|(weight, member)| (weight.clamp(1, 10), normalise(member, next_id)))
					.collect();
				let total: u32 = members.iter().map(|(weight, _)| weight).sum();
				WeightedGroup { threshold: 1 + threshold % total, members }
			},
			_ => {
				*next_id += 1;
				Individual { id: *next_id }
			},
		}
	}

	fn valid_council() -> impl Strategy<Value = Council<u64>> {
		(0u64..24)
			.prop_map(|id| Individual { id })
			.prop_recursive(2, 64, 6, |inner| {
				prop_oneof![
					(0u8..8, prop::collection::vec(inner.clone(), 0..7))
						.prop_map(|(threshold, members)| SimpleGroup { threshold, members }),
					(0u32..40, prop::collection::vec((0u32..10, inner), 0..7))
						.prop_map(|(threshold, members)| WeightedGroup { threshold, members }),
				]
			})
			.prop_map(|council| normalise(council, &mut 0))
	}

	fn subset(members: &BTreeSet<u64>, mask: u64) -> BTreeSet<u64> {
		members
			.iter()
			.enumerate()
			.filter(|(i, _)| mask.checked_shr(*i as u32).unwrap_or(0) & 1 == 1)
			.map(|(_, id)| *id)
			.collect()
	}

	proptest! {
		/// The core safety property: nothing that passes validation can be unfulfillable, trivially
		/// satisfied, too deep, too large, or double-count an account.
		#[test]
		fn validated_councils_are_well_formed(council in arb_council()) {
			if council.validate().is_ok() {
				prop_assert!(council.quorum_reached(&council.members()));
				prop_assert!(!council.quorum_reached(&BTreeSet::new()));
				prop_assert!(depth(&council) <= MAX_DEPTH);
				prop_assert!(council.members().len() <= MAX_MEMBERS);
				prop_assert_eq!(leaves(&council).len(), council.members().len());
			}
		}

		#[test]
		fn normalised_councils_validate(council in valid_council()) {
			prop_assert_eq!(council.validate(), Ok(()));
			prop_assert!(council.quorum_reached(&council.members()));
			prop_assert!(!council.quorum_reached(&BTreeSet::new()));
		}

		#[test]
		fn quorum_is_monotonic(council in valid_council(), mask in any::<u64>(), extra in any::<u64>()) {
			let members = council.members();
			let approvals = subset(&members, mask);
			if council.quorum_reached(&approvals) {
				let more: BTreeSet<u64> = approvals.union(&subset(&members, extra)).cloned().collect();
				prop_assert!(council.quorum_reached(&more));
			}
		}

		#[test]
		fn non_members_are_ignored(council in valid_council(), mask in any::<u64>(), outsiders in prop::collection::btree_set(1_000u64..2_000, 0..8)) {
			let approvals = subset(&council.members(), mask);
			let with_outsiders: BTreeSet<u64> = approvals.union(&outsiders).cloned().collect();
			prop_assert_eq!(council.quorum_reached(&approvals), council.quorum_reached(&with_outsiders));
			for outsider in &outsiders {
				prop_assert!(!council.is_member(outsider));
			}
		}
	}
}
