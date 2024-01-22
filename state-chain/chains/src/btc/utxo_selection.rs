use sp_std::{vec, vec::Vec};

use super::{BitcoinFeeInfo, ConsolidationParameters, Utxo};

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
pub fn select_utxos_from_pool(
	available_utxos: &mut Vec<Utxo>,
	fee_info: &BitcoinFeeInfo,
	amount_to_be_spent: u64,
) -> Option<(Vec<Utxo>, u64)> {
	if available_utxos.is_empty() {
		return None
	}

	available_utxos.sort_by_key(|utxo| sp_std::cmp::Reverse(utxo.amount));

	let mut selected_utxos: Vec<Utxo> = vec![];
	let mut skipped_utxos: Vec<Utxo> = vec![];

	let mut cumulative_amount = 0;

	while cumulative_amount < amount_to_be_spent {
		if let Some(current_smallest_utxo) = available_utxos.pop() {
			if current_smallest_utxo.amount > fee_info.fee_per_utxo(&current_smallest_utxo) {
				cumulative_amount +=
					current_smallest_utxo.amount - fee_info.fee_per_utxo(&current_smallest_utxo);
				selected_utxos.push(current_smallest_utxo.clone());
			} else {
				skipped_utxos.push(current_smallest_utxo.clone());
			}
		} else {
			return None
		}
	}

	if let Some(utxo) = available_utxos.pop() {
		cumulative_amount += utxo.amount - fee_info.fee_per_utxo(&utxo);
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
		.partition::<Vec<_>, _>(|utxo| utxo.amount > fee_info.fee_per_utxo(utxo));

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
	use crate::btc::{deposit_address::DepositAddress, UtxoId};

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

	let available_utxos: Vec<Utxo> = [1100u64, 250, 5000, 80, 150, 200, 190, 410, 10000, 7680]
		.iter()
		.zip(0u32..)
		.map(|x| build_utxo(*x.0, x.1))
		.collect();

	#[track_caller]
	fn test_case(
		initial_available_utxos: &[Utxo],
		fee_info: &BitcoinFeeInfo,
		amount_to_be_spent: u64,
		expected_selection: Option<(Vec<Utxo>, u64)>,
	) {
		let mut utxos = initial_available_utxos.to_owned();
		let selected = select_utxos_from_pool(&mut utxos, fee_info, amount_to_be_spent);

		assert_eq!(selected, expected_selection);

		// check remaining utxos:
		if let Some(expected) = expected_selection {
			assert_eq!(initial_available_utxos.len() - expected.0.len(), utxos.len());
			for utxo in initial_available_utxos {
				if selected.clone().unwrap().0.contains(utxo) {
					assert!(!utxos.contains(utxo));
				} else {
					assert!(utxos.contains(utxo));
				}
			}
		} else {
			assert_eq!(utxos, initial_available_utxos);
		}
	}

	let fee_info = BitcoinFeeInfo { sats_per_kilobyte: 1000 };

	// Empty utxo list as input should return Option::None.
	test_case(&Vec::<Utxo>::new(), &fee_info, 0, None);

	// Entering the amount greater than the max spendable amount will
	// cause the function to return no utxos. Note that we don't check
	// remaining utxos in this case since it will be an "incorrect" value
	// (which is OK since it will be ignored).
	assert_eq!(select_utxos_from_pool(&mut available_utxos.clone(), &fee_info, 1000000), None);

	test_case(
		&available_utxos,
		&fee_info,
		1,
		Some((vec![build_utxo(80, 3), build_utxo(150, 4)], 74)),
	);

	test_case(
		&available_utxos,
		&fee_info,
		18,
		Some((vec![build_utxo(80, 3), build_utxo(150, 4), build_utxo(190, 6)], 186)),
	);

	test_case(
		&available_utxos,
		&fee_info,
		80,
		Some((
			vec![build_utxo(80, 3), build_utxo(150, 4), build_utxo(190, 6), build_utxo(200, 5)],
			308,
		)),
	);

	let mut all_utxos_sorted = available_utxos.clone();
	all_utxos_sorted.sort_by_key(|utxo| utxo.amount);

	// The amount that will cause all utxos to be selected
	test_case(&available_utxos, &fee_info, 20000, Some((all_utxos_sorted.clone(), 24300)));

	// Max amount that can be spent with the given utxos.
	test_case(&available_utxos, &fee_info, 24300, Some((all_utxos_sorted, 24300)));

	// choosing the fee to spend the input utxo as greater than the amounts in the 2 smallest
	// utxos will cause the algorithm to skip the selection of those 2 utxos and adding it to the
	// list of available utxos for future use.
	test_case(
		&available_utxos,
		&BitcoinFeeInfo { sats_per_kilobyte: 2000 },
		190,
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
