use itertools::Itertools;
use sp_std::{vec, vec::Vec};

use super::{BitcoinFeeInfo, ConsolidationParameters, Utxo};

/// Attempt to select up to `selection_limit` number of uxtos that contains more than required
/// amount. Prioritize small amounts firs to avoid fragmenting.
///
/// In the case where the fee to spend the utxo is higher than the amount locked in the utxo, the
/// algorithm will skip the selection of that utxo and will keep it in the list of available utxos
/// for future use when the fee possibly comes down so that it is feasible to select these utxos.
///
/// The algorithm for the utxo selection works as follows:
/// 1. Filter out all available utxos whose fees are >= amount, they are excluded from the
/// selection.
///
/// 2. Check that in the worst case scenario, when the highest N utxos are selected, enough funds
/// can be accrued.
/// Return early if unable to select enough funds within the selection limit.
///
/// 3. In a greedy approach it starts selecting utxos from the lowest value utxos in a sorted array.
/// It keeps selecting the utxos until a) the cumulative amount in utxos (without fees) is just
/// greater than or equal to the total required amount to be egressed or b) when the number of
/// selected utxos has reached the given `selection_limit`.
///
/// 4. In the case that the selected utxos do not contain enough value, the algorithm will then
/// proceed to swap the smallest selected utxo with the largest unselected utxo, increasing the
/// total selected values without changing the number of utxos selected.
///
/// In the worst case scenarios, only the largest N utxos are selected. Checks in step 2 guarantees
/// enough value are selected.
///
/// 5. We then take 1 more utxo as safety measure (in the case that the fees are more expensive than
/// expected).
/// The skipped utxos are appended back to the `available_utxos` for future use.
///
/// Note that on failure (when `None` is returned) this may still modify `available_utxos`. It is
/// expected that the user will roll back storage on failed cases.
pub fn select_utxos_from_pool(
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	amount_to_be_spent: u64,
	selection_limit: u32,
) -> Option<(Vec<Utxo>, u64)> {
	if available_utxos.is_empty() {
		return None
	}

	// 1. Filter out utxos whose fees are too high.
	let mut skipped_utxos = available_utxos
		.extract_if(|utxo| utxo.amount <= fee_info.fee_for_utxo(utxo))
		.collect_vec();

	// 2. Return None if the largest N utxos cannot produce enough outputs.
	available_utxos.sort_by_key(|utxo| sp_std::cmp::Reverse(utxo.amount));
	if available_utxos
		.iter()
		.take(selection_limit as usize)
		.map(|utxo| utxo.amount.saturating_sub(fee_info.fee_for_utxo(utxo)))
		.sum::<u64>() <
		amount_to_be_spent
	{
		available_utxos.append(&mut skipped_utxos);
		return None
	}

	let mut selected_utxos: Vec<Utxo> = vec![];

	// 3. Select from the smallest utxos until either we have enough funds or reached the limit.
	let mut cumulative_amount = 0;
	while cumulative_amount < amount_to_be_spent && selected_utxos.len() < selection_limit as usize
	{
		if let Some(current_utxo) = available_utxos.pop() {
			cumulative_amount +=
				current_utxo.amount.saturating_sub(fee_info.fee_for_utxo(&current_utxo));
			selected_utxos.push(current_utxo.clone());
		} else {
			// This should never happen, but is here for safety.
			// Upon failure, it is expected to rollback storage
			return None
		}
	}

	// 4. If total amount is not reached, keep swapping smallest elected with the largest
	// unselected.
	// Since the largest N element is > target, worse case scenario is to select largest N utxos.
	while cumulative_amount < amount_to_be_spent &&
		!available_utxos.is_empty() &&
		!selected_utxos.is_empty()
	{
		// selected is sorted small >>> large
		// available is sorted large >>> small
		let small_utxo = selected_utxos.remove(0);
		let large_utxo = available_utxos.remove(0);

		if small_utxo.amount >= large_utxo.amount {
			// Guard against infinite loop. Cannot select enough utxo for the given amount.
			// Theoretically this should never happen, since we already checked against this
			// previously.
			return None
		}
		cumulative_amount = cumulative_amount +
			(large_utxo.amount - fee_info.fee_for_utxo(&large_utxo)) -
			(small_utxo.amount - fee_info.fee_for_utxo(&small_utxo));

		selected_utxos.push(large_utxo);
		available_utxos.push(small_utxo);
	}

	// 5. We are guaranteed to have enough funds here. Take 1 more after sufficient utxos are
	// selected.
	if let Some(utxo) = available_utxos.pop() {
		cumulative_amount += utxo.amount - fee_info.fee_for_utxo(&utxo);
		selected_utxos.push(utxo);
	}

	available_utxos.append(&mut skipped_utxos);

	Some((selected_utxos, cumulative_amount))
}

pub fn select_utxos_for_consolidation(
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	params: ConsolidationParameters,
) -> Vec<Utxo> {
	let (mut spendable, mut dust) = available_utxos
		.drain(..)
		.partition::<Vec<_>, _>(|utxo| utxo.amount > fee_info.fee_for_utxo(utxo));

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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::btc::{deposit_address::DepositAddress, UtxoId};
	use sp_std::collections::btree_set::BTreeSet;

	// Helper functions for testing utxo selection.
	// For simplicity each utxos must have distinct values.
	#[track_caller]
	fn test_case(
		initial_available_utxos: &[Utxo],
		fee_info: &BitcoinFeeInfo,
		amount_to_be_spent: u64,
		limit: Option<u32>,
		expected_selection: Option<(Vec<Utxo>, u64)>,
	) {
		let mut utxos = initial_available_utxos.to_owned();
		let selection_limit = limit.unwrap_or(utxos.len() as u32);
		let selected =
			select_utxos_from_pool(&mut utxos, fee_info, amount_to_be_spent, selection_limit);
		match (selected, expected_selection) {
			(Some(actual), Some(expected)) => {
				let expected_set =
					expected.0.iter().map(|utxo| utxo.amount).collect::<BTreeSet<_>>();
				let actual_set = actual.0.iter().map(|utxo| utxo.amount).collect::<BTreeSet<_>>();

				// Ensure no duplicate utxos are selected
				assert_eq!(expected.0.len(), expected_set.len());
				assert_eq!(actual.0.len(), actual_set.len());

				// Compare selection result, ignoring order.
				assert_eq!((actual_set, actual.1), (expected_set, expected.1));

				// Ensure the unselected utxos are kept in `initial_available_utxos`.
				assert_eq!(initial_available_utxos.len() - expected.0.len(), utxos.len());
				for utxo in initial_available_utxos {
					if actual.0.contains(utxo) {
						assert!(!utxos.contains(utxo));
					} else {
						assert!(utxos.contains(utxo));
					}
				}
			},
			(None, None) => {
				// If no result is returned, the utxo is only sorted, but otherwise unchanged.
				assert_eq!(
					utxos.iter().map(|utxo| utxo.amount).collect::<BTreeSet::<_>>(),
					initial_available_utxos
						.iter()
						.map(|utxo| utxo.amount)
						.collect::<BTreeSet::<_>>()
				);
			},
			(actual, expected) => panic!(
				"Test case failed. Actual utxo selected: \n{:?} \nExpected: \n{:?}",
				actual, expected
			),
		};
	}

	fn build_utxo(amount: u64, salt: u32) -> Utxo {
		Utxo {
			id: UtxoId::default(),
			amount,
			deposit_address: DepositAddress::new(
				hex_literal::hex!(
					"0000111122223333444455556666777788889999AAAABBBBCCCCDDDDEEEEFFFF"
				),
				salt,
			),
		}
	}

	fn mock_utxos() -> Vec<Utxo> {
		[1100u64, 250u64, 5000u64, 80u64, 150u64, 200u64, 190u64, 410u64, 10000u64, 7680u64]
			.iter()
			.zip(0u32..)
			.map(|x| build_utxo(*x.0, x.1))
			.collect()
	}

	#[test]
	fn test_utxo_selection_no_limit() {
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1000 };

		// Empty utxo list as input should return Option::None.
		test_case(&Vec::<Utxo>::new(), &fee_info, 0, None, None);

		// Entering the amount greater than the max spendable amount will
		// cause the function to return no utxos. Note that we don't check
		// remaining utxos in this case since it will be an "incorrect" value
		// (which is OK since it will be ignored).
		assert_eq!(select_utxos_from_pool(&mut mock_utxos(), &fee_info, 1000000, 100u32), None);

		test_case(
			&mock_utxos(),
			&fee_info,
			1,
			None,
			Some((vec![build_utxo(80, 3), build_utxo(150, 4)], 74)),
		);

		test_case(
			&mock_utxos(),
			&fee_info,
			18,
			None,
			Some((vec![build_utxo(80, 3), build_utxo(150, 4), build_utxo(190, 6)], 186)),
		);

		test_case(
			&mock_utxos(),
			&fee_info,
			80,
			None,
			Some((
				vec![build_utxo(80, 3), build_utxo(150, 4), build_utxo(190, 6), build_utxo(200, 5)],
				308,
			)),
		);

		let mut all_utxos_sorted = mock_utxos();
		all_utxos_sorted.sort_by_key(|utxo| utxo.amount);

		// The amount that will cause all utxos to be selected
		test_case(&mock_utxos(), &fee_info, 20000, None, Some((all_utxos_sorted.clone(), 24300)));

		// Max amount that can be spent with the given utxos.
		test_case(&mock_utxos(), &fee_info, 24300, None, Some((all_utxos_sorted, 24300)));

		// choosing the fee to spend the input utxo as greater than the amounts in the 2 smallest
		// utxos will cause the algorithm to skip the selection of those 2 utxos and adding it to
		// the list of available utxos for future use.
		test_case(
			&mock_utxos(),
			&BitcoinFeeInfo { sats_per_kilobyte: 2000 },
			190,
			None,
			Some((
				vec![
					build_utxo(190, 6),
					build_utxo(200, 5),
					build_utxo(250, 1),
					build_utxo(410, 7),
					build_utxo(1100, 0),
				],
				1410,
			)),
		);
	}

	#[test]
	fn test_utxo_selection_with_limit() {
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1000 };

		// Enable to select with the given limit.
		test_case(&mock_utxos(), &fee_info, 23_780, Some(3), None);

		// Enable to select due to high fees
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000_000 };
		test_case(&mock_utxos(), &fee_info, 1, Some(3), None);

		// lowest 5 are skipped due to high fee.
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 4_000 };
		test_case(
			&mock_utxos(),
			&fee_info,
			1_500,
			Some(3),
			Some((
				vec![
					build_utxo(410, 7),
					build_utxo(1_100, 0),
					build_utxo(5_000, 2),
					build_utxo(7_680, 8),
				],
				13_022,
			)),
		);

		test_case(
			&mock_utxos(),
			&fee_info,
			80,
			Some(5),
			Some((vec![build_utxo(410, 7), build_utxo(1_100, 0)], 966)),
		);

		// Lowest N do not have enough funds, swapping with larger utxo is required.
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 0 };
		test_case(
			&mock_utxos(),
			&fee_info,
			430,
			Some(3),
			Some((
				vec![
					build_utxo(150, 4),
					build_utxo(190, 6),
					build_utxo(10_000, 9),
					build_utxo(80, 3),
				],
				10_420,
			)),
		);

		// Worse case scenario: Highest N elements are selected
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000 };
		test_case(
			&mock_utxos(),
			&fee_info,
			22_000,
			Some(3),
			Some((
				vec![
					build_utxo(10_000, 9),
					build_utxo(7_680, 8),
					build_utxo(5_000, 2),
					build_utxo(190, 3),
				],
				22_558,
			)),
		);
	}
}
