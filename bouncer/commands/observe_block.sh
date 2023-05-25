#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument.
// It will observe the chainflip state-chain until the block with the blocknumber given by the argument
// is observed

// For example: ./commands/observe_block.sh 3
// will wait until block number 3 has appeared on the state chain

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	var cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	const expected_block = process.argv[2];
	const api = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});
	while((await api.rpc.chain.getBlockHash(expected_block)).every(e => {return e == 0;})){
		await new Promise(r => setTimeout(r, 1000));
	}
	console.log("Observed block no. " + expected_block);
	process.exit(0);
}

runWithTimeout(main(), 10000).catch((error) => {
	console.log("Failed to observe block no. " + process.argv[2]);
	process.exit(-1);
});