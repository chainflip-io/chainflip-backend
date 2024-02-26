use sp_std::{vec, vec::Vec};

use super::{BitcoinFeeInfo, Utxo, UtxoParameters};

#[derive(Debug, PartialEq, Eq)]
pub enum UtxoSelectionError {
	/// All available utxos have their Fees > amount, therefore is unusable.
	NoUtxoAvailableAfterFeeReduction,
	/// Cannot select utxos with enough funds within the given limit.
	InsufficientFundsInAvailableUtxos,
}

/// Attempt to select up to `selection_limit` number of uxtos that contains more than required
/// amount. Prioritize small amounts first to avoid fragmentation.
///
/// On success, the `(selected_utxos and accumulated_total)` is returned.
/// On failure the appropriate error is returned, and `available_utxos` is still modified. It is
/// expected that the user will roll back storage.
///
/// In the case where the fee to spend the utxo is higher than the amount locked in the utxo, the
/// algorithm will skip the selection of that utxo and will keep it in the list of available utxos
/// for future use when the fee possibly comes down so that it is feasible to select these utxos.
///
/// The algorithm for the utxo selection works as follows:
/// 1. Filter out all available utxos whose fees are >= amount, they are excluded from the
/// selection.
///
/// 2. Find a optimal range such that `sum(utxo[first ..= last]) >= target_amount`
/// When the selected range is < selection_limit, move the `first` index to the left
///
/// e.g. using a selection limit of 3
/// Currently 2 utxos are selected. Move the start index to the left.
///                    | end
/// 10 9 8 7 6 5 4 3 2 1
///                |<| Start
/// New end points to "1" and new start points to "3"
///
/// When the selection limit is reached, both the start and end pointer are moved.
///                  |<| end
/// 10 9 8 7 6 5 4 3 2 1
///            |<| Start
/// New end points to 2 and new start points to 5, keeping the selected utxos to the limit of 3.
///
/// In the worst case scenarios, the largest N utxos are selected.
///
/// If after this step, the accumulated amount is still below the target, report as failure.
/// Thought this is extremely unlikely to happen since we proactively consolidate our utxos.
///
/// 3. We then take 1 more utxo if within limit, to actively reduce fragmentation.
/// The skipped utxos are appended back to the `available_utxos` for future use.
pub fn select_utxos_from_pool(
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	amount_to_be_spent: u64,
	maybe_limit: Option<u32>,
) -> Result<(Vec<Utxo>, u64), UtxoSelectionError> {
	// If no selection limit is given, selecting all utxos is allowed.
	let selection_limit = match maybe_limit {
		Some(limit) => limit as usize,
		None => available_utxos.len(),
	};

	// 1. Filter out utxos whose fees are too high.
	let mut skipped_utxos = available_utxos
		.extract_if(|utxo| utxo.amount <= fee_info.fee_for_utxo(utxo))
		.collect::<Vec<_>>();

	if available_utxos.is_empty() || selection_limit == 0usize {
		return Err(UtxoSelectionError::NoUtxoAvailableAfterFeeReduction)
	}

	// 2. Find the optimal `first` and `last` index, such that
	// `sum(utxo[first ..= last]) >= target_amount`
	available_utxos.sort_by_key(|utxo| sp_std::cmp::Reverse(utxo.amount));

	let mut first = available_utxos.len();
	let mut last = first - 1;
	let mut cumulative_amount = 0;

	while first > 0usize {
		first -= 1usize;
		let utxo_to_add = &available_utxos[first];
		cumulative_amount += utxo_to_add.amount.saturating_sub(fee_info.fee_for_utxo(utxo_to_add));

		if last - first + 1 > selection_limit {
			// Move the `last` pointer forward by one to keep selection size within limit.
			let utxo_to_remove = &available_utxos[last];
			last -= 1;
			cumulative_amount -=
				utxo_to_remove.amount.saturating_sub(fee_info.fee_for_utxo(utxo_to_remove));
		}

		if cumulative_amount >= amount_to_be_spent {
			break;
		}
	}

	if cumulative_amount < amount_to_be_spent {
		// Failed to find utxos that contained target amount - extremely unlikely since we
		// proactively consolidate out utxos.
		Err(UtxoSelectionError::InsufficientFundsInAvailableUtxos)
	} else {
		// 3. Try to fit one more utxo in
		if first > 0usize && last - first + 1 < selection_limit {
			first -= 1usize;
			let utxo = &available_utxos[first];
			cumulative_amount += utxo.amount - fee_info.fee_for_utxo(utxo);
		}

		// Take all utxos from the `first` to `last` (inclusive).
		let selected_utxos = available_utxos.splice(first..=last, []).collect();

		// Re-append the skipped utxos since they are not used.
		available_utxos.append(&mut skipped_utxos);

		Ok((selected_utxos, cumulative_amount))
	}
}

pub fn select_utxos_for_consolidation(
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	params: UtxoParameters,
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
		maybe_limit: Option<u32>,
		expected: Result<(Vec<u64>, u64), UtxoSelectionError>,
	) {
		let mut utxos = initial_available_utxos.to_owned();
		let selected =
			select_utxos_from_pool(&mut utxos, fee_info, amount_to_be_spent, maybe_limit);
		match (selected, expected) {
			(Ok(actual), Ok(expected)) => {
				let actual_set = actual.0.iter().map(|utxo| utxo.amount).collect::<BTreeSet<_>>();
				let expected_set = BTreeSet::from_iter(expected.0.clone());

				// Ensure no duplicate utxos are selected
				assert_eq!(actual.0.len(), actual_set.len());
				assert_eq!(expected.0.len(), expected_set.len());

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
			(Err(actual), Err(expected)) => assert_eq!(actual, expected),
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
		test_case(
			&Vec::<Utxo>::new(),
			&fee_info,
			0,
			None,
			Err(UtxoSelectionError::NoUtxoAvailableAfterFeeReduction),
		);

		// Entering the amount greater than the max spendable amount will
		// cause the function to return no utxos. Note that we don't check
		// remaining utxos in this case since it will be an "incorrect" value
		// (which is OK since it will be ignored).
		assert_eq!(
			select_utxos_from_pool(&mut mock_utxos(), &fee_info, 1000000, Some(100u32)),
			Err(UtxoSelectionError::InsufficientFundsInAvailableUtxos)
		);

		test_case(&mock_utxos(), &fee_info, 1, None, Ok((vec![80, 150], 74)));

		test_case(&mock_utxos(), &fee_info, 18, None, Ok((vec![80, 150, 190], 186)));

		test_case(&mock_utxos(), &fee_info, 80, None, Ok((vec![80, 150, 190, 200], 308)));

		let all_utxos = mock_utxos().into_iter().map(|utxo| utxo.amount).collect::<Vec<_>>();

		// The amount that will cause all utxos to be selected
		test_case(&mock_utxos(), &fee_info, 20000, None, Ok((all_utxos.clone(), 24300)));

		// Max amount that can be spent with the given utxos.
		test_case(&mock_utxos(), &fee_info, 24300, None, Ok((all_utxos, 24300)));

		// choosing the fee to spend the input utxo as greater than the amounts in the 2 smallest
		// utxos will cause the algorithm to skip the selection of those 2 utxos and adding it to
		// the list of available utxos for future use.
		test_case(
			&mock_utxos(),
			&BitcoinFeeInfo { sats_per_kilobyte: 2000 },
			190,
			None,
			Ok((vec![190, 200, 250, 410, 1_100], 1410)),
		);
	}

	#[test]
	fn test_utxo_selection_with_limit() {
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1000 };

		// Unable to select with the given limit.
		test_case(
			&mock_utxos(),
			&fee_info,
			23_780,
			Some(3),
			Err(UtxoSelectionError::InsufficientFundsInAvailableUtxos),
		);

		// Unable to select due to high fees
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000_000 };
		test_case(
			&mock_utxos(),
			&fee_info,
			1,
			Some(3),
			Err(UtxoSelectionError::NoUtxoAvailableAfterFeeReduction),
		);

		// lowest 5 utxos are skipped due to high fee.
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 4_000 };
		test_case(&mock_utxos(), &fee_info, 1_500, Some(3), Ok((vec![410, 1_100, 5_000], 5_654)));

		test_case(&mock_utxos(), &fee_info, 80, Some(5), Ok((vec![410, 1_100], 966)));

		// Lowest N do not have enough funds. `last` pointer need to be shifted up.
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 0 };
		test_case(&mock_utxos(), &fee_info, 430, Some(3), Ok((vec![150, 190, 200], 540)));

		// Worst case scenario: Highest N elements are selected
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000 };
		test_case(
			&mock_utxos(),
			&fee_info,
			22_000,
			Some(3),
			Ok((vec![10_000, 7_680, 5_000], 22_446)),
		);
	}
}
