.PHONY: build-chainspec-sisyphos
build-chainspec-sisyphos:
	./target/release/chainflip-node build-spec --chain sisyphos-new --disable-default-bootnode > state-chain/node/chainspecs/sisyphos.chainspec.json
	./target/release/chainflip-node build-spec --chain sisyphos-new --disable-default-bootnode --raw > state-chain/node/chainspecs/sisyphos.chainspec.raw.json

.PHONY: build-chainspec-perseverance
build-chainspec-perseverance:
	./target/release/chainflip-node build-spec --chain perseverance --disable-default-bootnode > state-chain/node/chainspecs/perseverance.chainspec.json
	./target/release/chainflip-node build-spec --chain perseverance --raw --disable-default-bootnode > state-chain/node/chainspecs/perseverance.chainspec.raw.json
