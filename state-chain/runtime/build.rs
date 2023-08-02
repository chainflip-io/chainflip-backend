fn main() {
	#[cfg(feature = "std")]
	{
		use substrate_wasm_builder::WasmBuilder;
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.build();
	}
}
