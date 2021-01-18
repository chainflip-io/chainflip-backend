# Setting up Docker Compose

First, generate a `docker-compose.yml` file using the `gen-chain-docker-compose.sh` utility script. As arguments, pass valid `--name` arguments (alice, bob, charlie etc... see `state-chain-node --help` for a full list). The script should output a valid docker-compose config to `stdout`. Pipe this to a custom `docker-compose` configuration file:

```bash
./gen-chain-docker-compose.sh alice bob eve > docker-compose.yml
```

You can then start the network. 

```bash
docker-compose up
```

This will start the nodes but if you look closely you'll notice they can't connect to each other yet! Sadly, substrate's autodiscovery feature doesn't work on docker networks. Not to worry. 

Hit `ctrl-C` (or type `docker-compose stop` in another terminal) to stop the chain and look for the following line (or similar) near the start of the log:

```
cf-substrate-node-alice_1    | Jan 15 14:34:29.715INFO 🏷  Local node identity is: 12D3KooWJo19xzLH4QFxCo8YE6ZHbA9L8SH6MZbLGaWRC4UZLQj5
```

Note which of the named nodes this ID corresponds to (*Alice*, in this case) - we will make this our bootstrap node. 

Now, open the `docker-compose` config generated above and for each of the *other* nodes in the config, add `--bootnodes /ip4/${bootnode-ip-address}/tcp/30333/p2p/${bootnode-peer-id}` to the end of the command, replacing `${boootnode-ip-address}` and `${bootnode-peer-id}` with the bootstrap node's ip and the id from the log. It should look like this: 

```yaml
  command: ./target/release/state-chain-node --dev --ws-external --eve --bootnodes /ip4/172.28.0.2/tcp/30333/p2p/12D3KooWJo19xzLH4QFxCo8YE6ZHbA9L8SH6MZbLGaWRC4UZLQj5
```

Save this file and run `docker-compose up` again, and the nodes should connect! Something like this:
```
cf-substrate-node-eve_1    | Jan 15 16:17:31.244  INFO 🔍 Discovered new external address for our node: /ip4/172.28.0.4/tcp/30333/p2p/12D3KooWBKts6C3EJ1vs3w1LVb7gBQKwkaUQX8ayqHB1kWPEQW3d
cf-substrate-node-alice_1  | Jan 15 16:17:31.283  INFO 🔍 Discovered new external address for our node: /ip4/172.28.0.2/tcp/30333/p2p/12D3KooWJo19xzLH4QFxCo8YE6ZHbA9L8SH6MZbLGaWRC4UZLQj5
cf-substrate-node-bob_1    | Jan 15 16:17:31.343  INFO 🔍 Discovered new external address for our node: /ip4/172.28.0.3/tcp/30333/p2p/12D3KooWBnDtEXqzuydeBjnb4ttxiYeCVmKz4rFHHhFwuuFH9KcR
cf-substrate-node-alice_1  | Jan 15 16:17:35.070  INFO 💤 Idle (2 peers), best: #6 (0xacb0…b916), finalized #4 (0x6cb8…32f0), ⬇ 1.3kiB/s ⬆ 2.7kiB/s
cf-substrate-node-eve_1    | Jan 15 16:17:35.698  INFO 💤 Idle (2 peers), best: #6 (0xacb0…b916), finalized #4 (0x6cb8…32f0), ⬇ 2.5kiB/s ⬆ 1.7kiB/s
cf-substrate-node-bob_1    | Jan 15 16:17:35.789  INFO 💤 Idle (2 peers), best: #6 (0xacb0…b916), finalized #4 (0x6cb8…32f0), ⬇ 2.5kiB/s ⬆ 1.7kiB/s
```

### Finally...

We are now ready to interact via the admin interface. 

Open another terminal in the same directory and run `docker container ls` to see a list of port mappings for port 9944. You can use the mapped ports to connect via the standard [polkadot app](https://polkadot.js.org/apps). 