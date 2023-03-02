use crate::{vec, Vec};

#[derive(Clone)]
pub struct UTXO {
	pub amount: u64,
}

#[allow(dead_code)]
fn select_utxos_from_pool(
	mut available_utxos: Vec<UTXO>,
	fee_per_utxo: u64,
	amount_to_be_egressed: u64,
) -> Vec<UTXO> {
	if amount_to_be_egressed == 0 {
		return vec![]
	}

	// Sort the utxos by the amounts they hold, in descending order
	available_utxos.sort_by_key(|utxo| utxo.clone().amount);

	let mut selected_utxos: Vec<UTXO> = vec![];

	let mut cumulative_amount = 0;

	// Start selecting the utxos from the smallest amount and keep on selecting the utxos until the
	// cummulative amount of all selected utxos (plus the fees that need to be paid on spending
	// them) just exceeds the total amount that needs to be spent (such that not selecting the last
	// utxo would reduce the cummulative amount below the required amount).
	while cumulative_amount < amount_to_be_egressed {
		let current_smallest_utxo = available_utxos.pop().expect("The funds in vault should be greater than the amount requested to be egressed. This is made sure elsewhere and should be expected here");
		cumulative_amount += current_smallest_utxo.clone().amount - fee_per_utxo;
		selected_utxos.push(current_smallest_utxo);
	}

	// Select one more utxo that is next in line (smallest) among the remaining unselected utxos.
	// Dont select any the utxo in case there is none remianing.
	if let Some(utxo) = available_utxos.pop() {
		selected_utxos.push(utxo);
	}

	selected_utxos
}
