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
