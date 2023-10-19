# About

Ingress-Egress Tracker observes events on external blockchains (ETH, DOT, BTC) and provides a way for client applications to subscribe and receive
these events via a WebSocket subscription. For BTC, the tracker exposes a separate RPC call to query transactions in the mempool in addition to the
WebSocket subscription.

# Setup

The tracker will start an RPC server on 0.0.0.0:13337 (at the moment this cannot be configured).
When working with a "localnet" (e.g. for development purposes), no extra configuration is necessary: `./chainflip-ingress-egress-tracker`.
The default configuration can be overwritten with the following env variables:

```
- ETH_WS_ENDPOINT: Ethereum node websocket endpoint. (Default: ws://localhost:8546)
- ETH_HTTP_ENDPOINT: Ethereum node http endpoint. (Default: http://localhost:8545)
- DOT_WS_ENDPOINT: Polkadot node websocket endpoint. (Default: ws://localhost:9945)
- DOT_HTTP_ENDPOINT: Polkadot node http endpoint. (Default: http://localhost:9945)
- SC_WS_ENDPOINT: Chainflip node websocket endpoint. (Default: ws://localhost:9944)
- BTC_ENDPOINT: Bitcoin node http endpoint. (Default: http://127.0.0.1:8332)
- BTC_USERNAME: Bitcoin node username. (Default: flip)
- BTC_PASSWORD: Bitcoin node password. (Default: flip)
```

# Usage

Using a WebSocket client, here is an example interaction with the tracker (`wscat`` is used in this case):
 
1. The tracker is started with no parameters, which will connect it to localnet:
 
```
./chainflip-ingress-egress-tracker
```

2. wscat is used to connect the tracker's endpoint and subscribe to the witnessing stream using `subscribe_witnessing` method:

```
> wscat -c ws://0.0.0.0:13337
> {"jsonrpc":"2.0","id":0,"method":"subscribe_witnessing"}
```

3. To create a "witnessable" event, we execute a swap on localnet using bouncer:

```
commands/perform_swap.ts flip btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX
```

4. Shortly wscat should receive subscription result and two events (corresponding to ingress and egress transactions):

```
< {"jsonrpc":"2.0","result":4146820711520716,"id":0}
< {"jsonrpc":"2.0","method":"s_witnessing","params":{"subscription":6933028418422314,"result":[32,2,4,42,245,64,173,248,154,105,209,51,45,107,31,67,57,202,174,35,169,195,59,1,0,0,80,239,226,214,228,26,27,0,0,0,0,0,0,0,70,3,0,0,0,0,0,0]}}
< {"jsonrpc":"2.0","method":"s_witnessing","params":{"subscription":6933028418422314,"result":[29,2,233,156,159,177,49,75,198,4,61,48,118,36,65,90,173,49,235,19,68,245,52,174,124,128,236,198,52,168,160,48,156,97,4,113,86,64,189,104,54,243,89,38,22,25,220,64,95,198,192,249,231,43,50,187,126,21,43,174,148,99,185,58,31,157,175,0,0,0,0,0,0,0,0]}}
```

The events are substrate's SCALE codec encoded and correspond to the following decoded events, respectively:

```
RuntimeCall::EthereumIngressEgress(Call::process_deposits { deposit_witnesses: [DepositWitness { deposit_address: 0x2af540adf89a69d1332d6b1f4339caae23a9c33b, asset: Flip, amount: 500000000000000000000, deposit_details: () }], block_height: 838 })
```

```
RuntimeCall::BitcoinBroadcaster(Call::transaction_succeeded { tx_out_id: [233, 156, 159, 177, 49, 75, 198, 4, 61, 48, 118, 36, 65, 90, 173, 49, 235, 19, 68, 245, 52, 174, 124, 128, 236, 198, 52, 168, 160, 48, 156, 97], signer_id: Taproot([113, 86, 64, 189, 104, 54, 243, 89, 38, 22, 25, 220, 64, 95, 198, 192, 249, 231, 43, 50, 187, 126, 21, 43, 174, 148, 99, 185, 58, 31, 157, 175]), tx_fee: 0 })
```