#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Dot balance of the address provided as the first argument.
//
// For example: ./commands/get_dot_balance.sh 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci
// might print: 1.2

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const address = process.argv[2] || '0';
	const polkadot_endpoint = process.env.POLKADOT_ENDPOINT || 'ws://127.0.0.1:9945';
	const polkadot = await ApiPromise.create({provider: new WsProvider(polkadot_endpoint), noInitWarn: true});
	const planck_balance = (await polkadot.query.system.account(address)).data.free.toString();
	const balance_len = planck_balance.length;
	var balance;
	if(balance_len > 10){
		const decimal_location = balance_len - 10;
		balance = planck_balance.substr(0, decimal_location) + '.' + planck_balance.substr(decimal_location);
	} else {
		balance = "0." + planck_balance.padStart(10, '0');
	}
	console.log(balance);
	process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
	console.error(error);
	process.exit(-1);
});