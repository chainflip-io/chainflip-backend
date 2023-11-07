use super::ConsolidationParameters;
use crate::GetAmount;
use frame_support::sp_runtime::traits::AtLeast32BitUnsigned;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

/// The algorithm for the utxo selection works as follows: In a greedy approach it starts selecting
/// utxos from the lowest value utxos in a sorted array. It keeps selecting the utxos until the
/// cumulative amount in utxos is just greater than or equal to the total amount to be egressed
/// plus fees of spending the utxos such that not including the last utxo would have the cumulative
/// amount fall below the required. It then includes one more utxo if it is available.
/// This approach is provably non-fragmenting. Specifically, it can be proven that the minimum
/// amount utxo in the list of available utxos after the transaction is greater than the minimum
/// amount utxo in the list before the transaction EXCEPT for the case where the algorithm has to
/// choose all available utxos for the transaction but then the fragmentation doesn't matter anyways
/// since we in any case have to use all utxos (because the output amount is high enough).
///
/// In the case where the fee to spend the utxo is higher than the amount locked in the utxo, the
/// algorithm will skip the selection of that utxo and will keep it in the list of available utxos
/// for future use when the fee possibly comes down so that it is feasible to select these utxos.
///
/// Note that on failure (when `None` is returned) this may still modify `available_utxos`, which
/// is not a concern because the caller will ignore that value in that case.
pub fn select_utxos_from_pool<A: AtLeast32BitUnsigned + Copy, UTXO: GetAmount<A> + Ord + Clone>(
	available_utxos: &mut BTreeSet<UTXO>,
	fee_per_utxo: A,
	amount_to_be_spent: A,
) -> Option<(Vec<UTXO>, A)> {
	if available_utxos.is_empty() {
		return None
	}

	let mut withdrawn = Vec::default();
	let mut skipped = Vec::default();
	let mut total = A::zero();

	while total < amount_to_be_spent {
		let utxo = available_utxos.pop_first()?;
		if utxo.amount() < fee_per_utxo {
			skipped.push(utxo);
		} else {
			total += utxo.amount() - fee_per_utxo;
			withdrawn.push(utxo);
		}
	}

	if let Some(utxo) = available_utxos.pop() {
		cumulative_amount += utxo.amount() - fee_per_utxo;
		selected_utxos.push(utxo);
	}
	for utxo in skipped {
		available_utxos.insert(utxo);
	}

	Some((withdrawn, total))
}

pub fn select_utxos_for_consolidation<UTXO: GetUtxoAmount + Clone>(
	available_utxos: &mut Vec<UTXO>,
	fee_per_utxo: u64,
	params: ConsolidationParameters,
) -> Vec<UTXO> {
	let (mut spendable, mut dust) = available_utxos
		.drain(..)
		.partition::<Vec<_>, _>(|utxo| utxo.amount() > fee_per_utxo);

	if spendable.len() >= params.consolidation_threshold as usize {
		let mut remaining = spendable.split_off(params.consolidation_size as usize);
		// put remaining and dust back:
		available_utxos.append(&mut remaining);
		available_utxos.append(&mut dust);
		spendable
	} else {
		// Not strictly necessary (the caller is expected roll back the change), but
		// let's put everything back just as a precaution:
		available_utxos.append(&mut spendable);
		available_utxos.append(&mut dust);
		vec![]
	}
}

#[test]
fn test_utxo_selection() {
	use super::*;
	use std::collections::BTreeSet;

	#[allow(clippy::upper_case_acronyms)]
	type UTXO = u64;

	const FEE_PER_UTXO: u64 = 2;

	let available_utxos = vec![110, 25, 500, 7, 15, 20, 19, 41, 1000, 768];

	#[track_caller]
	fn test_case(
		initial_available_utxos: &[UTXO],
		fee_per_utxo: u64,
		amount_to_be_spent: u64,
		expected_selection: Option<(Vec<UTXO>, u64)>,
	) {
		let mut utxos = initial_available_utxos.iter().cloned().collect();
		let selected = select_utxos_from_pool(&mut utxos, fee_per_utxo, amount_to_be_spent);

		assert_eq!(selected, expected_selection);

		// check remaining utxos:
		let initial = initial_available_utxos.iter().collect::<BTreeSet<_>>();
		let selected = selected.map(|x| x.0).unwrap_or_default();
		let selected = selected.iter().collect::<BTreeSet<_>>();
		let remaining = utxos.iter().collect::<BTreeSet<_>>();

		assert_eq!(
			initial.difference(&selected).copied().collect::<BTreeSet<_>>(),
			remaining,
			"remaining utxos do not match"
		);
	}

	// Empty utxo list as input should return Option::None.
	test_case(&Vec::<UTXO>::new(), FEE_PER_UTXO, 0, None);

	// Entering the amount greater than the max spendable amount will
	// cause the function to return no utxos. Note that we don't check
	// remaining utxos in this case since it will be an "incorrect" value
	// (which is OK since it will be ignored).
	assert_eq!(
		select_utxos_from_pool(
			&mut available_utxos.clone().into_iter().collect(),
			FEE_PER_UTXO,
			100000
		),
		None
	);

	test_case(&available_utxos, FEE_PER_UTXO, 1, Some((vec![7, 15], 18)));

	test_case(&available_utxos, FEE_PER_UTXO, 18, Some((vec![7, 15, 19], 35)));

	test_case(&available_utxos, FEE_PER_UTXO, 19, Some((vec![7, 15, 19, 20], 53)));

	let all_selected_utxos = {
		let mut utxos = available_utxos.clone();
		utxos.sort();
		utxos
	};

	// The amount that will cause all utxos to be selected
	test_case(&available_utxos, FEE_PER_UTXO, 2000, Some((all_selected_utxos.clone(), 2485)));

	// Max amount that can be spent with the given utxos.
	test_case(&available_utxos, FEE_PER_UTXO, 2485, Some((all_selected_utxos, 2485)));

	// choosing the fee to spend the input utxo as greater than the amounts in the 2 smallest
	// utxos will cause the algorithm to skip the selection of those 2 utxos and adding it to the
	// list of available utxos for future use.
	test_case(&available_utxos, 16, 19, Some((vec![19, 20, 25, 41, 110], 135)));
}
