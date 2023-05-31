#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes four arguments.
// It will request a new swap with the provided parameters
// Argument 1 is the source currency ("btc", "eth", "dot" or "usdc")
// Argument 2 is the destination currency ("btc", "eth", "dot" or "usdc")
// Argument 3 is the destination address
// Argument 4 is the broker fee in basis points
// For example: ./commands/new_swap.sh dot btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX 100

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { u8aToHex } = require('@polkadot/util');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const broker_uri = process.env.BROKER_URI || '//BROKER_1';
	const broker = keyring.createFromUri(broker_uri);
	const chainflip = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});
	const source_ccy = process.argv[2];
	const destination_ccy = process.argv[3];
	const destination_address = destination_ccy == 'dot' ? u8aToHex(keyring.decodeAddress(process.argv[4])) : process.argv[4];
	const fee = process.argv[5];

	console.log("Requesting Swap " + source_ccy + " -> " + destination_ccy);
	let result = await chainflip.tx.swapping.requestSwapDepositAddress(source_ccy, destination_ccy, {[destination_ccy == 'usdc' ? 'eth' : destination_ccy]: destination_address}, fee, null).signAndSend(broker);
	process.exit(0);
}

runWithTimeout(main(), 60000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
