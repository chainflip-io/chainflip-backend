use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;
use sp_std::vec::Vec;

use super::{BitcoinFeeInfo, BtcAmount, Utxo};

#[derive(Debug, PartialEq, Eq)]
pub enum UtxoSelectionError {
	/// All available utxos have their Fees > amount, therefore is unusable.
	NoUtxoAvailableAfterFeeReduction,
	/// Cannot select utxos with enough funds within the given limit.
	InsufficientFundsInAvailableUtxos,
}

#[derive(Encode, Decode, Default, PartialEq, Copy, Clone, TypeInfo, RuntimeDebug)]
pub struct ConsolidationParameters {
	/// Consolidate when total UTXO count reaches this threshold
	pub consolidation_threshold: u32,
	/// Consolidate this many UTXOs
	pub consolidation_size: u32,
}

impl ConsolidationParameters {
	#[cfg(test)]
	fn new(consolidation_threshold: u32, consolidation_size: u32) -> ConsolidationParameters {
		ConsolidationParameters { consolidation_threshold, consolidation_size }
	}

	pub fn are_valid(&self) -> bool {
		self.consolidation_size <= self.consolidation_threshold && self.consolidation_size > 1
	}
}

/// Attempt to select up to `selection_limit` number of uxtos that contains more than required
/// amount. Prioritize small amounts first to avoid fragmentation.
///
/// On success, the `(selected_utxos and accumulated_total)` is returned and available_utxos is
/// modified, the selected utxos removed.
///
/// In the error case, the available_utxos may *also* be modified, it is expected that the caller
/// would not persist the available_utxos in the error case.
///
/// The algorithm for the utxo selection works as follows:
///
/// 1. Sort the available utxos according to their net value (amount - fees)
///
/// 2. Initialize the `first` and `last` index to the first utxo with net value > 0.
///
/// 3. Find a contiguous set of fewer than `selection_limit` utxos such that the total value exceeds
///    the target amount.
///
/// 4. If there are still few utxos than the limit, add one more utxo. This prevents fragmentation
///    of the utxo set.
///
/// In the worst case scenario, the largest N utxos are selected.
///
/// An error is return if:
/// - There is are no utxos with positive net value.
/// - It is not possible to select enough contiguous utxos to meet the target amount.
pub fn select_utxos_from_pool(
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	amount_to_be_spent: BtcAmount,
	maybe_limit: Option<u32>,
) -> Result<(Vec<Utxo>, BtcAmount), UtxoSelectionError> {
	// If no selection limit is given, selecting all utxos is allowed.
	let selection_limit = match maybe_limit {
		Some(limit) => limit as usize,
		None => available_utxos.len(),
	};

	available_utxos.sort_by_key(|utxo| utxo.net_value(fee_info));

	let mut last = available_utxos
		.iter()
		.position(|utxo| utxo.net_value(fee_info) > 0u64)
		.ok_or(UtxoSelectionError::NoUtxoAvailableAfterFeeReduction)?;
	let mut first = last;
	let mut cumulative_amount = 0;

	while last < available_utxos.len() {
		cumulative_amount += available_utxos[last].net_value(fee_info);

		if last - first >= selection_limit {
			// Move the `first` pointer forward by one to keep selection size within limit.
			cumulative_amount -= available_utxos[first].net_value(fee_info);
			first += 1usize;
		}

		if cumulative_amount >= amount_to_be_spent {
			break;
		}
		last += 1usize;
	}

	if cumulative_amount < amount_to_be_spent {
		Err(UtxoSelectionError::InsufficientFundsInAvailableUtxos)
	} else {
		if last < available_utxos.len() - 1 && last - first + 1 < selection_limit {
			last += 1usize;
			cumulative_amount += available_utxos[last].net_value(fee_info);
		}

		// Take all utxos from the `first` to `last` (inclusive).
		Ok((available_utxos.splice(first..=last, []).collect(), cumulative_amount))
	}
}

/// Attempt to select Utxos to ne consolidated into the current vault.
/// There are 2 situation where utxos need to be consolidated:
/// 1. When the number of Utxos available is >= `consolidation_threshold` - here we consolidate the
///    utxos into the current vault, reducing the number of utxos and future fees.
/// 2. When there are utxos from the previous vault. We consolidate these utxos to the current
///    vault.
///
/// Utxos that have a zero (or below) net value (amount - fees) are ignored. They will eventually
/// become stale if the fees do not come down, and is discarded when they are no longer usable.
pub fn select_utxos_for_consolidation(
	current_key: [u8; 32],
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	consolidation_parameter: ConsolidationParameters,
) -> Vec<Utxo> {
	// filter out utxos whose net value is 0
	let (mut spendable, mut dust) = available_utxos
		.drain(..)
		.partition::<Vec<_>, _>(|utxo| utxo.net_value(fee_info) > 0u64);

	if spendable.len() >= consolidation_parameter.consolidation_threshold as usize {
		// Take a limited number of utxos to be consolidated. Return the rest to storage.
		let mut remaining =
			spendable.split_off(consolidation_parameter.consolidation_size as usize);

		// put remaining and dust back:
		available_utxos.append(&mut remaining);
		available_utxos.append(&mut dust);
		spendable
	} else {
		// Transfer old utxos into the current vault. Remaining utxos must be > consolidation_size.
		// Utxos are pre-filtered. Only utxos that belong to current or previous vaults are
		// remaining.
		let mut prev_utxos = spendable
			.extract_if(|utxo: &mut Utxo| utxo.deposit_address.pubkey_x != current_key)
			.collect::<Vec<_>>();

		let mut remaining = prev_utxos.split_off(sp_std::cmp::min(
			prev_utxos.len(),
			consolidation_parameter.consolidation_size as usize,
		));

		available_utxos.append(&mut spendable);
		available_utxos.append(&mut remaining);
		available_utxos.append(&mut dust);

		prev_utxos
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
		amount_to_be_spent: BtcAmount,
		maybe_limit: Option<u32>,
		expected: Result<(Vec<BtcAmount>, BtcAmount), UtxoSelectionError>,
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

	fn build_utxo(amount: BtcAmount, salt: u32, key: Option<[u8; 32]>) -> Utxo {
		Utxo {
			id: UtxoId::default(),
			amount,
			deposit_address: DepositAddress::new(
				key.unwrap_or(hex_literal::hex!(
					"0000111122223333444455556666777788889999AAAABBBBCCCCDDDDEEEEFFFF"
				)),
				salt,
			),
		}
	}

	fn mock_utxos() -> Vec<Utxo> {
		[1100u64, 250u64, 5000u64, 80u64, 150u64, 200u64, 190u64, 410u64, 10000u64, 7680u64]
			.iter()
			.zip(0u32..)
			.map(|x| build_utxo(*x.0, x.1, None))
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
			Ok((vec![5_000, 7_680, 10_000], 22_446)),
		);
	}

	#[test]
	fn consolidation_parameters() {
		// These are expected to be valid:
		assert!(ConsolidationParameters::new(2, 2).are_valid());
		assert!(ConsolidationParameters::new(10, 2).are_valid());
		assert!(ConsolidationParameters::new(10, 10).are_valid());
		assert!(ConsolidationParameters::new(200, 100).are_valid());
		assert!(ConsolidationParameters::new(2, 2).are_valid());

		// Invalid: size < threshold
		assert!(!ConsolidationParameters::new(9, 10).are_valid());

		// Invalid: size is too small
		assert!(!ConsolidationParameters::new(0, 0).are_valid());
		assert!(!ConsolidationParameters::new(1, 1).are_valid());
		assert!(!ConsolidationParameters::new(0, 10).are_valid());
	}

	#[test]
	fn test_consolidation_path_1() {
		let key1 = [0xAA; 32];
		let key2 = [0xAA; 32];
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000 };
		let parameter = ConsolidationParameters::new(4, 2);

		let mut utxos = vec![
			build_utxo(1, 0, Some(key1)),
			build_utxo(1_000, 0, Some(key1)),
			build_utxo(2_000, 0, Some(key2)),
			build_utxo(3_000, 0, Some(key1)),
			build_utxo(4_000, 0, Some(key2)),
		];
		assert_eq!(
			select_utxos_for_consolidation(key1, &mut utxos, &fee_info, parameter,),
			vec![build_utxo(1_000, 0, Some(key1)), build_utxo(2_000, 0, Some(key2))]
		);
		assert_eq!(
			utxos,
			vec![
				build_utxo(3_000, 0, Some(key1)),
				build_utxo(4_000, 0, Some(key2)),
				build_utxo(1, 0, Some(key1)),
			]
		);
	}

	#[test]
	fn test_consolidation_no_consolidation() {
		let key1 = [0xAA; 32];
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000 };
		let parameter = ConsolidationParameters::new(10, 2);

		let mut utxos = vec![
			build_utxo(1, 0, Some(key1)),
			build_utxo(1_000, 0, Some(key1)),
			build_utxo(2_000, 0, Some(key1)),
			build_utxo(3_000, 0, Some(key1)),
			build_utxo(4_000, 0, Some(key1)),
		];
		assert_eq!(select_utxos_for_consolidation(key1, &mut utxos, &fee_info, parameter,), vec![]);

		assert_eq!(
			utxos,
			vec![
				build_utxo(1_000, 0, Some(key1)),
				build_utxo(2_000, 0, Some(key1)),
				build_utxo(3_000, 0, Some(key1)),
				build_utxo(4_000, 0, Some(key1)),
				build_utxo(1, 0, Some(key1)),
			]
		);
	}

	#[test]
	fn test_consolidation_partial_previous_utxos() {
		let key1 = [0xAA; 32];
		let key2 = [0xBB; 32];
		let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1_000 };
		let parameter = ConsolidationParameters::new(10, 2);

		let mut utxos = vec![
			build_utxo(1, 0, Some(key1)),
			build_utxo(1_000, 0, Some(key1)),
			build_utxo(2_000, 0, Some(key2)),
			build_utxo(3_000, 0, Some(key2)),
			build_utxo(4_000, 0, Some(key2)),
		];
		assert_eq!(
			select_utxos_for_consolidation(key1, &mut utxos, &fee_info, parameter,),
			vec![build_utxo(2_000, 0, Some(key2)), build_utxo(3_000, 0, Some(key2)),]
		);

		assert_eq!(
			utxos,
			vec![
				build_utxo(1_000, 0, Some(key1)),
				build_utxo(4_000, 0, Some(key2)),
				build_utxo(1, 0, Some(key1)),
			]
		);
	}
}
