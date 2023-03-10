// The algorithm for the utxo selection works as follows: In a greedy approach it starts selecting
// utxos from the lowest value utxos in a sorted array. It keeps selecting the utxos until the
// cummulative amount in utxos is just greater than or equal to the total amount to be egressed plus
// fees of spending the utxos such that not including the last utxo would have the cummulative
// amount fall below the required. It then includes one more utxo if it is available.
// This approach is provably non-fragmenting. Specifically, it can be proven that the minimum
// amount utxo in the list of available utxos after the transaction is greater than the minimum
// amount utxo in the list before the transaction EXCEPT for the case where the algorithm has to
// choose all available utxos for the transaction but then the fragmentation doesnt matter anyways
// since we in any case have to use all utxos (because the output amount is high enough).

use sp_std::{vec, vec::Vec};

use super::GetUtxoAmount;

pub fn select_utxos_from_pool<UTXO: GetUtxoAmount>(
	available_utxos: &mut Vec<UTXO>,
	fee_per_utxo: u64,
	amount_to_be_egressed: u64,
) -> Option<(Vec<UTXO>, u64)> {
	if amount_to_be_egressed == 0 || available_utxos.is_empty() {
		return None
	}

	// Sort the utxos by the amounts they hold, in descending order
	available_utxos.sort_by_key(|utxo| utxo.amount());
	available_utxos.reverse();

	let mut selected_utxos: Vec<UTXO> = vec![];

	let mut cumulative_amount = 0;

	// Start selecting the utxos from the smallest amount and keep on selecting the utxos until the
	// cummulative amount of all selected utxos (plus the fees that need to be paid on spending
	// them) just exceeds the total amount that needs to be spent (such that not selecting the last
	// utxo would reduce the cummulative amount below the required amount). If all available utxos
	// have been selected and the egress amount is still not met, select all utxos and return.
	while cumulative_amount < amount_to_be_egressed {
		if let Some(current_smallest_utxo) = available_utxos.pop() {
			cumulative_amount += current_smallest_utxo.amount() - fee_per_utxo;
			selected_utxos.push(current_smallest_utxo);
		} else {
			break
		}
	}

	// Select one more utxo that is next in line (smallest) among the remaining unselected utxos.
	// Dont select any the utxo in case there is none remianing.
	if let Some(utxo) = available_utxos.pop() {
		cumulative_amount += utxo.amount() - fee_per_utxo;
		selected_utxos.push(utxo);
	}

	Some((selected_utxos, cumulative_amount))
}

#[test]
fn test_utxo_selection() {
	use super::GetUtxoAmount;

	#[allow(clippy::upper_case_acronyms)]
	#[derive(Clone, Debug, PartialEq, Eq)]
	pub struct UTXO {
		pub amount: u64,
	}
	impl GetUtxoAmount for UTXO {
		fn amount(&self) -> u64 {
			self.amount
		}
	}

	const FEEPERUTXO: u64 = 2;

	let mut available_utxos = vec![
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

	// empty list is output for 0 egress
	assert_eq!(select_utxos_from_pool(&mut available_utxos.clone(), FEEPERUTXO, 0), None);
	assert_eq!(
		select_utxos_from_pool(&mut available_utxos.clone(), FEEPERUTXO, 1),
		Some((vec![UTXO { amount: 7 }, UTXO { amount: 15 }], 18))
	);
	assert_eq!(
		select_utxos_from_pool(&mut available_utxos.clone(), FEEPERUTXO, 18),
		Some((vec![UTXO { amount: 7 }, UTXO { amount: 15 }, UTXO { amount: 19 }], 35))
	);
	assert_eq!(
		select_utxos_from_pool(&mut available_utxos.clone(), FEEPERUTXO, 19),
		Some((
			vec![UTXO { amount: 7 }, UTXO { amount: 15 }, UTXO { amount: 19 }, UTXO { amount: 20 }],
			53
		))
	);
	// The amount that will cause all utxos to be selected
	assert_eq!(
		select_utxos_from_pool(&mut available_utxos.clone(), FEEPERUTXO, 2000),
		Some((
			vec![
				UTXO { amount: 7 },
				UTXO { amount: 15 },
				UTXO { amount: 19 },
				UTXO { amount: 20 },
				UTXO { amount: 25 },
				UTXO { amount: 41 },
				UTXO { amount: 110 },
				UTXO { amount: 500 },
				UTXO { amount: 768 },
				UTXO { amount: 1000 },
			],
			2485
		))
	);
	// max amount that can be spent with the given utxos.
	assert_eq!(
		select_utxos_from_pool(&mut available_utxos.clone(), FEEPERUTXO, 2485),
		Some((
			vec![
				UTXO { amount: 7 },
				UTXO { amount: 15 },
				UTXO { amount: 19 },
				UTXO { amount: 20 },
				UTXO { amount: 25 },
				UTXO { amount: 41 },
				UTXO { amount: 110 },
				UTXO { amount: 500 },
				UTXO { amount: 768 },
				UTXO { amount: 1000 },
			],
			2485
		))
	);
	// entering the amount greater than the max spendable amount will
	// cause the function to select all available utxos
	assert_eq!(
		select_utxos_from_pool(&mut available_utxos, FEEPERUTXO, 100000),
		Some((
			vec![
				UTXO { amount: 7 },
				UTXO { amount: 15 },
				UTXO { amount: 19 },
				UTXO { amount: 20 },
				UTXO { amount: 25 },
				UTXO { amount: 41 },
				UTXO { amount: 110 },
				UTXO { amount: 500 },
				UTXO { amount: 768 },
				UTXO { amount: 1000 },
			],
			2485
		))
	);
}
