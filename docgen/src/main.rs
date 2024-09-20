use std::io::BufReader;

use rustdoc_json::PackageTarget;
use rustdoc_types::{Item, ItemEnum, ItemKind, Trait};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	for bin in ["chainflip-lp-api", "chainflip-broker-api"] {
		let doc_path = rustdoc_json::Builder::default()
			.toolchain("nightly")
			.manifest_path(format!("../api/bin/{bin}/Cargo.toml"))
			.document_private_items(false)
			.package_target(PackageTarget::Bin(format!("{}", bin)))
			.quiet(true)
			.target_dir("./json")
			.build()?;

		println!("Wrote rustdoc JSON to {:?}", &doc_path);

		let doc_crate: rustdoc_types::Crate =
			serde_json::from_reader(BufReader::new(std::fs::File::open(doc_path)?))?;

		// TODO
		// - Find the top-level RpcServer trait
		// - Find all the methods on that trait
		// - For each method
		//   - Get the input and output types (aka. request / response)
		//   - Get the doc string
		//   - Parse the doc string into markdown
		//   - Check that the doc contains a JSON section (or similar). If not, error.
		//   - Check that the JSON section contains
		// - LATER
		//   - Use a package such as schemars and its JsonSchema trait
		//   - schemars can also be use to derive a schema from an example value

		let ids = doc_crate
			.index
			.iter()
			.filter_map(|(_id, item)| match item {
				Item { id, name: Some(name), inner: ItemEnum::Trait(Trait { .. }), .. }
					if name == "RpcServer" =>
					Some(id),
				_ => None,
			})
			.collect::<Vec<_>>();

		println!("Found: {:?}", ids);
	}

	Ok(())
}
