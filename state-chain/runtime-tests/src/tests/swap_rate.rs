use super::*;
use cf_primitives::Asset;
use codec::{Decode, Encode};
use sc_executor::WasmExecutor;

#[derive(Debug, Default)]
pub struct Test;

impl RuntimeTest for Test {
	fn run(self, _block_hash: state_chain_runtime::Hash, mut ext: Ext) -> anyhow::Result<()> {
		let input_asset = Asset::ArbUsdc;
		let output_asset = Asset::SolUsdc;
		let input_amount = 1_000_000_000u128;

		// --- Native execution (uses compiled runtime code) ---
		let native_result = ext.execute_with(|| {
			state_chain_runtime::chainflip::simulate_swap::simulate_swap(
				input_asset,
				output_asset,
				input_amount,
				0,
				None,
				None,
				Default::default(),
				None,
				None,
			)
		});
		println!("Native result: {:?}", native_result);

		// --- WASM execution (uses the on-chain :code blob) ---
		let wasm_code = ext.execute_with(|| {
			sp_io::storage::get(b":code").expect(":code not found in state")
		});
		println!("WASM blob size: {} bytes", wasm_code.len());

		let runtime_blob =
			sc_executor_common::runtime_blob::RuntimeBlob::uncompress_if_needed(&wasm_code)
				.expect("Failed to decompress WASM blob");

		let call_data = (
			input_asset,
			output_asset,
			input_amount,
			0u16, // broker_commission
			Option::<cf_primitives::DcaParameters>::None,
			Option::<state_chain_runtime::runtime_apis::types::CcmData>::None,
			std::collections::BTreeSet::<
				state_chain_runtime::runtime_apis::types::FeeTypes,
			>::new(),
			Option::<
				Vec<state_chain_runtime::runtime_apis::types::SimulateSwapAdditionalOrder>,
			>::None,
			Option::<bool>::None,
		)
			.encode();

		let executor = WasmExecutor::<sp_io::SubstrateHostFunctions>::builder().build();
		let mut state_ext = ext.ext();
		let wasm_result = executor.uncached_call(
			runtime_blob,
			&mut state_ext,
			true, // allow_missing_host_functions
			"CustomRuntimeApi_cf_pool_simulate_swap",
			&call_data,
		);
		drop(state_ext);

		match wasm_result {
			Ok(output) => {
				let decoded = Result::<
					state_chain_runtime::runtime_apis::types::SimulatedSwapInformation,
					state_chain_runtime::runtime_apis::types::DispatchErrorWithMessage,
				>::decode(&mut &output[..]);
				println!("WASM result:   {:?}", decoded);
			},
			Err(e) => {
				println!("WASM execution error: {:?}", e);
			},
		}

		Ok(())
	}
}
