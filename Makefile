.PHONY: build-chainspec-sisyphos
build-chainspec-sisyphos:
	cargo cf-build-ci
	./target/release/chainflip-node build-spec --chain sisyphos-new --disable-default-bootnode > state-chain/node/chainspecs/sisyphos.chainspec.json
	./target/release/chainflip-node build-spec --chain state-chain/node/chainspecs/sisyphos.chainspec.json --disable-default-bootnode --raw > state-chain/node/chainspecs/sisyphos.chainspec.raw.json

.PHONY: build-chainspec-perseverance
build-chainspec-perseverance:
	cargo cf-build-ci
	./target/release/chainflip-node build-spec --chain perseverance-new --disable-default-bootnode > state-chain/node/chainspecs/perseverance.chainspec.json
	./target/release/chainflip-node build-spec --chain ./state-chain/node/chainspecs/perseverance.chainspec.json --raw --disable-default-bootnode > state-chain/node/chainspecs/perseverance.chainspec.raw.json

.PHONY: build-chainspec-partnernet
build-chainspec-partnernet:
	cargo cf-build-ci
	./target/release/chainflip-node build-spec --chain partnernet-new --disable-default-bootnode > state-chain/node/chainspecs/partnernet.chainspec.json
	./target/release/chainflip-node build-spec --chain ./state-chain/node/chainspecs/partnernet.chainspec.json --raw --disable-default-bootnode > state-chain/node/chainspecs/partnernet.chainspec.raw.json
