.PHONY: build-chainspec-backspin
build-chainspec-backspin:
	cargo build
	. localnet/init/env/arb.env && \
	. localnet/init/env/eth.env && \
	. localnet/init/env/1-node/eth-aggkey.env && \
	. localnet/init/env/1-node/dot-aggkey.env && \
	. localnet/init/env/arb.env && \
	. localnet/init/env/cfe.env && \
	. localnet/init/env/node.env && \
	. localnet/init/env/secrets.env && \
	./target/release/chainflip-node build-spec --chain dev --disable-default-bootnode > state-chain/node/chainspecs/backspin.chainspec.json
	./target/release/chainflip-node build-spec --chain state-chain/node/chainspecs/backspin.chainspec.json --disable-default-bootnode --raw > state-chain/node/chainspecs/backspin.chainspec.raw.json

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
