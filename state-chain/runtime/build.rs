fn main() {
	#[cfg(all(feature = "std", feature = "metadata-hash"))]
	{
		substrate_wasm_builder::WasmBuilder::new()
			.with_current_project()
			.export_heap_base()
			.import_memory()
			.enable_metadata_hash("FLIP", 18)
			.build();
	}
}
