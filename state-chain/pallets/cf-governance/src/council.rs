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

use codec::{Compact, Decode, DecodeWithMemTracking, Encode, Input};
use frame_support::{ensure, PalletError};
use scale_info::TypeInfo;
use sp_std::{collections::btree_set::BTreeSet, mem::size_of, vec::Vec};

pub type VoteWeight = u32;

/// Counts an `Individual` as depth 1, so this allows root group -> sub-group -> individual.
pub const MAX_DEPTH: u32 = 3;
/// Bounds the cost of re-evaluating the quorum on every approval.
pub const MAX_MEMBERS: usize = 64;

/// A (possibly nested) body whose members' approvals decide whether a proposal passes.
///
/// Approvals are tracked as a flat set of individual accounts: a group reaches quorum once enough
/// of its members do, counted by number (`SimpleGroup`) or by weight (`WeightedGroup`).
///
/// `Decode` is implemented by hand to bound the nesting and group size while decoding, so
/// untrusted input can't recurse up to the extrinsic depth limit or allocate oversized groups.
#[derive(Encode, DecodeWithMemTracking, TypeInfo, Clone, Debug, PartialEq, Eq)]
#[codec(encode_bound(AccountId: Encode))]
#[codec(decode_with_mem_tracking_bound(AccountId: DecodeWithMemTracking))]
#[scale_info(bounds(AccountId: TypeInfo + 'static))]
pub enum Council<AccountId> {
	#[codec(index = 0)]
	WeightedGroup { threshold: VoteWeight, members: Vec<(VoteWeight, Council<AccountId>)> },
	#[codec(index = 1)]
	SimpleGroup { threshold: u8, members: Vec<Council<AccountId>> },
	#[codec(index = 2)]
	Individual { id: AccountId },
}

impl<AccountId: Decode> Decode for Council<AccountId> {
	fn decode<I: Input>(input: &mut I) -> Result<Self, codec::Error> {
		Self::decode_at(input, 1)
	}
}

impl<AccountId: Decode> Council<AccountId> {
	/// Mirrors the derived encoding, rejecting shapes `validate` would reject anyway: nesting
	/// beyond `MAX_DEPTH` (counted the same way) and groups wider than `MAX_MEMBERS`.
	fn decode_at<I: Input>(input: &mut I, depth: u32) -> Result<Self, codec::Error> {
		if depth > MAX_DEPTH {
			return Err("Council is nested deeper than MAX_DEPTH".into())
		}
		let depth = depth.saturating_add(1);
		Ok(match input.read_byte()? {
			0 => Self::WeightedGroup {
				threshold: VoteWeight::decode(input)?,
				members: decode_members(input, |input| {
					Ok((VoteWeight::decode(input)?, Self::decode_at(input, depth)?))
				})?,
			},
			1 => Self::SimpleGroup {
				threshold: u8::decode(input)?,
				members: decode_members(input, |input| Self::decode_at(input, depth))?,
			},
			2 => Self::Individual { id: AccountId::decode(input)? },
			_ => return Err("Invalid Council variant".into()),
		})
	}
}

/// Decodes a `Vec` the way the codec does, after checking its length against `MAX_MEMBERS`.
fn decode_members<I: Input, T>(
	input: &mut I,
	mut decode_member: impl FnMut(&mut I) -> Result<T, codec::Error>,
) -> Result<Vec<T>, codec::Error> {
	let len = usize::try_from(Compact::<u32>::decode(input)?.0)
		.map_err(|_| "Council group length overflows usize")?;
	if len > MAX_MEMBERS {
		return Err("Council group has more than MAX_MEMBERS members".into())
	}
	input.descend_ref()?;
	input.on_before_alloc_mem(len.saturating_mul(size_of::<T>()))?;
	let members = (0..len).map(|_| decode_member(input)).collect();
	input.ascend_ref();
	members
}

/// Disables governance: with no members, quorum can never be reached. `validate` rejects it, so
/// only an empty genesis config can set it.
impl<AccountId> Default for Council<AccountId> {
	fn default() -> Self {
		Self::SimpleGroup { threshold: 1, members: Vec::new() }
	}
}

#[derive(
	Encode, Decode, DecodeWithMemTracking, TypeInfo, PalletError, Clone, Copy, Debug, PartialEq, Eq,
)]
pub enum InvalidCouncil {
	/// Nested deeper than `MAX_DEPTH`.
	TooDeep,
	/// More than `MAX_MEMBERS` members.
	TooManyMembers,
	/// A group has no members.
	EmptyGroup,
	/// A group has a threshold of zero, which would approve anything.
	ZeroThreshold,
	/// A member of a weighted group has zero weight.
	ZeroWeight,
	/// A group's weights overflow.
	WeightOverflow,
	/// A group's threshold exceeds its members' combined count or weight.
	ThresholdUnreachable,
	/// An account appears more than once.
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

	pub fn nested_council() -> Council<u64> {
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
	fn nested_council_quorum() {
		let council = nested_council();
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
		let council = nested_council();
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
	fn decoding_rejects_groups_wider_than_max_members() {
		let widest = Council::simple_group(1, 0..MAX_MEMBERS as u64);
		assert_eq!(Council::decode(&mut &widest.encode()[..]), Ok(widest));
		let too_wide = Council::<u64>::simple_group(1, 0..=MAX_MEMBERS as u64);
		assert!(Council::<u64>::decode(&mut &too_wide.encode()[..]).is_err());
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
		/// satisfied, or double-count an account. Samples rarely come near `MAX_DEPTH` or
		/// `MAX_MEMBERS`, so the unit tests and `too_deep_councils_are_rejected` pin the bounds.
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

		/// Exactly one level too deep, so an off-by-one in the depth check is caught.
		#[test]
		fn too_deep_councils_are_rejected(council in valid_council()) {
			// The wrappers are otherwise valid and the depth check fails before any member is
			// visited, so `TooDeep` is the only possible error.
			let too_deep = (depth(&council)..=MAX_DEPTH)
				.fold(council, |inner, _| SimpleGroup { threshold: 1, members: vec![inner] });
			prop_assert_eq!(depth(&too_deep), MAX_DEPTH + 1);
			prop_assert_eq!(too_deep.validate(), Err(TooDeep));
			prop_assert!(Council::<u64>::decode(&mut &too_deep.encode()[..]).is_err());
		}

		/// `arb_council` reaches one level past `MAX_DEPTH` but stays well under `MAX_MEMBERS`
		/// per group, so only the depth decides whether the round trip succeeds.
		#[test]
		fn decoding_round_trips_up_to_max_depth(council in arb_council()) {
			let decoded = Council::<u64>::decode(&mut &council.encode()[..]);
			if depth(&council) <= MAX_DEPTH {
				prop_assert_eq!(decoded, Ok(council));
			} else {
				prop_assert!(decoded.is_err());
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
