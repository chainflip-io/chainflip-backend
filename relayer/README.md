# Relayer

This component relays protocol events from Ethereum to the state chain. 

## StakeManager

The `StakeManager` contract is where prospective validators can stake FLIP tokens to participate in a validator auction.

Two type of event can be emitted: `Staked(nodeID,amount)` and `Claimed(nodeId,amount)`. 

# Testing

> TODO: Automate these tests!

For now, a basic sanity check is as follows:

1. Run a local state chain node: `./target/release/state-chain-node --dev --ws-external`.

2. Run a local ganache instance using mnemonic `chainflip` and the db stored in `relayer/tests/ganache-db/.test-chain`.

    With ganache installed locally:

    ```sh
    ganache-cli --mnemonic chainflip --db /db/.test-chain
    ```
  
    Or using docker:

    ```sh
    docker run -it \
        --mount type=bind,src=`pwd`/relayer/tests/ganache-db,dst=/db,ro \
        --publish 8545:8545 \
        trufflesuite/ganache-cli:latest \
            --mnemonic chainflip --db /db/.test-chain
    ```

    The provided db contains the `StakeManager` contract pre-loaded at address 
    `0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f` and with two `Staked` events triggered.

3. Run the relayer, giving the above servicee endpoints and the contract address as arguments:

    ```sh
    cargo build --bin relayer
    RUST_LOG=debug ./target/debug/relayer \
        ws://localhost:9944 \
        ws://localhost:8545 \
        0xEAd5De9C41543E4bAbB09f9fE4f79153c036044f
    ```

    You should see something like this:

    ```log
    [2021-02-26T15:16:07Z DEBUG relayer] Connecting to event source and sinks...
    [2021-02-26T15:16:07Z INFO  relayer] Starting relayer.
    [2021-02-26T15:16:07Z DEBUG web3::transports::ws] [1] Calling: {"jsonrpc":"2.0","method":"eth_getLogs","params":[{"address":"0xead5de9c41543e4babb09f9fe4f79153c036044f","fromBlock":"0x0"}],"id":1}
    [2021-02-26T15:16:07Z DEBUG web3::transports::ws] [2] Calling: {"jsonrpc":"2.0","method":"eth_subscribe","params":["logs",{"address":"0xead5de9c41543e4babb09f9fe4f79153c036044f","fromBlock":"pending"}],"id":2}
    [2021-02-26T15:16:07Z INFO  relayer::relayer::eth_event_streamer] Subscribed. Listening for events.
    [2021-02-26T15:16:07Z DEBUG relayer::relayer::contracts::stake_manager] Parsing event from block 8 with signature: 0x925435fa7e37e5d9555bb18ce0d62bb9627d0846942e58e5291e9a2dded462ed
    [2021-02-26T15:16:07Z INFO  relayer::relayer::sinks::logger] Received event: Staked(12321, 100000000000000000000000)
    [2021-02-26T15:16:07Z DEBUG relayer::relayer::sinks::state_chain] Encoded event call as: 09002130000000000000000000000000000000000000000000000000000000000000000080f64ae1c7022d15000000000000
    [2021-02-26T15:16:07Z DEBUG relayer::relayer::contracts::stake_manager] Parsing event from block 9 with signature: 0x925435fa7e37e5d9555bb18ce0d62bb9627d0846942e58e5291e9a2dded462ed
    [2021-02-26T15:16:07Z INFO  relayer::relayer::sinks::logger] Received event: Staked(45654, 100000000000000000000001)
    [2021-02-26T15:16:07Z DEBUG relayer::relayer::sinks::state_chain] Encoded event call as: 090056b2000000000000000000000000000000000000000000000000000000000000010080f64ae1c7022d15000000000000
    [2021-02-26T15:16:08Z INFO  relayer::relayer::sinks::state_chain] Extrinsic submitted, hash: 0x6d6ed437035af4b79cd040e46c0678355859916e5e28f3f1855c1d59f1baa9d1
    [2021-02-26T15:16:08Z INFO  relayer::relayer::sinks::state_chain] Extrinsic submitted, hash: 0x5a05844925c118faefeaab8516c8039a0607d8f1ec579fc53a6400f176784dd8
    ```
