.PHONY: build-chainspec-sisyphos
build-chainspec-sisyphos:
	cargo build --release
	./target/release/chainflip-node build-spec --chain sisyphos-new --disable-default-bootnode > state-chain/node/chainspecs/sisyphos.chainspec.json
	./target/release/chainflip-node build-spec --chain state-chain/node/chainspecs/sisyphos.chainspec.json --disable-default-bootnode --raw > state-chain/node/chainspecs/sisyphos.chainspec.raw.json

.PHONY: build-chainspec-perseverance
build-chainspec-perseverance:
	cargo build --release
	./target/release/chainflip-node build-spec --chain perseverance-new --disable-default-bootnode > state-chain/node/chainspecs/perseverance.chainspec.json
	./target/release/chainflip-node build-spec --chain ./state-chain/node/chainspecs/perseverance.chainspec.json --raw --disable-default-bootnode > state-chain/node/chainspecs/perseverance.chainspec.raw.json

.PHONY: build-chainspec-partnernet
build-chainspec-partnernet:
	cargo build --release
	./target/release/chainflip-node build-spec --chain partnernet-new --disable-default-bootnode > state-chain/node/chainspecs/partnernet.chainspec.json
	./target/release/chainflip-node build-spec --chain ./state-chain/node/chainspecs/partnernet.chainspec.json --raw --disable-default-bootnode > state-chain/node/chainspecs/partnernet.chainspec.raw.json

.PHONY: broker-docs
broker-docs:
	SKIP_WASM_BUILD= cargo build --release --bin chainflip-broker-api --bin api-docgen
	./target/release/chainflip-broker-api schema | jq > ./api/bin/chainflip-broker-api/schema.json
	./target/release/api-docgen --schema ./api/bin/chainflip-broker-api/schema.json --output ./api/bin/chainflip-broker-api/api-doc.md
