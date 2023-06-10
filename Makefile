.PHONY: build-chainspec-development
build-chainspec-development:
	cargo cf-build-ci
	./target/release/chainflip-node build-spec --chain dev --disable-default-bootnode > state-chain/node/chainspecs/development.chainspec.json
	./target/release/chainflip-node build-spec --chain state-chain/node/chainspecs/development.chainspec.json --disable-default-bootnode --raw > state-chain/node/chainspecs/development.chainspec.raw.json

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
