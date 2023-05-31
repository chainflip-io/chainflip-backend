#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new polkadot address and return the address
// For example: ./commands/new_dot_address.sh foobar
// returns: 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const polkadot_endpoint = process.env.POLKADOT_ENDPOINT || 'ws://127.0.0.1:9945';
	const seed = process.argv[2] || '0';
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const address = keyring.createFromUri('//' + seed).address;
	console.log(address);
	process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
	console.error(error);
	process.exit(-1);
});