use sp_std::{vec, vec::Vec};

use super::GetUtxoAmount;

/// The algorithm for the utxo selection works as follows: In a greedy approach it starts selecting
/// utxos from the lowest value utxos in a sorted array. It keeps selecting the utxos until the
/// cummulative amount in utxos is just greater than or equal to the total amount to be egressed
/// plus fees of spending the utxos such that not including the last utxo would have the cummulative
/// amount fall below the required. It then includes one more utxo if it is available.
/// This approach is provably non-fragmenting. Specifically, it can be proven that the minimum
/// amount utxo in the list of available utxos after the transaction is greater than the minimum
/// amount utxo in the list before the transaction EXCEPT for the case where the algorithm has to
/// choose all available utxos for the transaction but then the fragmentation doesnt matter anyways
/// since we in any case have to use all utxos (because the output amount is high enough).
///
/// In the case where the fee to spend the utxo is higher than the amount locked in the utxo, the
/// algorithm will skip the selection of that utxo and will keep it in the list of available utxos
/// for future use when the fee possibly comes down so that it is feasable to select these utxos.
pub fn select_utxos_from_pool<UTXO: GetUtxoAmount + Clone>(
	available_utxos_out: &mut Vec<UTXO>,
	fee_per_utxo: u64,
	amount_to_be_spent: u64,
) -> Option<(Vec<UTXO>, u64)> {
	if available_utxos_out.is_empty() {
		return None
	}

	let mut available_utxos = available_utxos_out.clone();

	available_utxos.sort_by_key(|utxo| sp_std::cmp::Reverse(utxo.amount()));

	let mut selected_utxos: Vec<UTXO> = vec![];
	let mut skipped_utxos: Vec<UTXO> = vec![];

	let mut cumulative_amount = 0;

	while cumulative_amount < amount_to_be_spent {
		if let Some(current_smallest_utxo) = available_utxos.pop() {
			if current_smallest_utxo.amount() > fee_per_utxo {
				cumulative_amount += current_smallest_utxo.amount() - fee_per_utxo;
				selected_utxos.push(current_smallest_utxo.clone());
			} else {
				skipped_utxos.push(current_smallest_utxo.clone());
			}
		} else {
			return None
		}
	}

	if let Some(utxo) = available_utxos.pop() {
		cumulative_amount += utxo.amount() - fee_per_utxo;
		selected_utxos.push(utxo);
	}

	available_utxos.append(&mut skipped_utxos);

	*available_utxos_out = available_utxos;
	Some((selected_utxos, cumulative_amount))
}

#[test]
fn test_utxo_selection() {
	use super::GetUtxoAmount;
	use std::collections::BTreeSet;

	#[allow(clippy::upper_case_acronyms)]
	#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
	pub struct UTXO {
		pub amount: u64,
	}
	impl GetUtxoAmount for UTXO {
		fn amount(&self) -> u64 {
			self.amount
		}
	}

	const FEE_PER_UTXO: u64 = 2;

	let available_utxos = vec![
		UTXO { amount: 110 },
		UTXO { amount: 25 },
		UTXO { amount: 500 },
		UTXO { amount: 7 },
		UTXO { amount: 15 },
		UTXO { amount: 20 },
		UTXO { amount: 19 },
		UTXO { amount: 41 },
		UTXO { amount: 1000 },
		UTXO { amount: 768 },
	];

	#[track_caller]
	fn test_case(
		initial_available_utxos: &Vec<UTXO>,
		fee_per_utxo: u64,
		amount_to_be_spent: u64,
		expected_selection: Option<(Vec<UTXO>, u64)>,
	) {
		let mut utxos = initial_available_utxos.clone();
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

	// empty utxo list as input should return Option::None.
	test_case(&Default::default(), 0, FEE_PER_UTXO, None);

	test_case(
		&available_utxos,
		FEE_PER_UTXO,
		1,
		Some((vec![UTXO { amount: 7 }, UTXO { amount: 15 }], 18)),
	);

	test_case(
		&available_utxos,
		FEE_PER_UTXO,
		18,
		Some((vec![UTXO { amount: 7 }, UTXO { amount: 15 }, UTXO { amount: 19 }], 35)),
	);

	test_case(
		&available_utxos,
		FEE_PER_UTXO,
		19,
		Some((
			vec![UTXO { amount: 7 }, UTXO { amount: 15 }, UTXO { amount: 19 }, UTXO { amount: 20 }],
			53,
		)),
	);

	let all_selected_utxos = {
		let mut utxos = available_utxos.clone();
		utxos.sort();
		utxos
	};

	// The amount that will cause all utxos to be selected
	test_case(&available_utxos, FEE_PER_UTXO, 2000, Some((all_selected_utxos.clone(), 2485)));

	// Max amount that can be spent with the given utxos.
	test_case(&available_utxos, FEE_PER_UTXO, 2485, Some((all_selected_utxos, 2485)));

	// Entering the amount greater than the max spendable amount will
	// cause the function to return no utxos
	test_case(&available_utxos, FEE_PER_UTXO, 100000, None);

	// choosing the fee to spend the input utxo as greater than the amounts in the 2 smallest
	// utxos will cause the algorithm to skip the selection of those 2 utxos and adding it to the
	// list of available utxos for future use.
	test_case(
		&available_utxos,
		16,
		19,
		Some((
			vec![
				UTXO { amount: 19 },
				UTXO { amount: 20 },
				UTXO { amount: 25 },
				UTXO { amount: 41 },
				UTXO { amount: 110 },
			],
			135,
		)),
	);
}
