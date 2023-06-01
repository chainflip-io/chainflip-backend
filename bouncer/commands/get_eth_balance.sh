#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Eth balance of the address provided as the first argument.
//
// For example: ./commands/get_eth_balance.sh 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 1.2

const Web3 = require('web3');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const eth_endpoint = process.env.ETH_ENDPOINT || "http://127.0.0.1:8545";
	const ethereum_address = process.argv[2] || '0';
	const web3 = new Web3(eth_endpoint);

	const wei_balance = await web3.eth.getBalance(ethereum_address);
	const balance_len = wei_balance.length;
	var balance;
	if(balance_len > 18){
		const decimal_location = balance_len - 18;
		balance = wei_balance.substr(0, decimal_location) + '.' + wei_balance.substr(decimal_location);
	} else {
		balance = "0." + wei_balance.padStart(18, '0');
	}
	console.log(balance);
	process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
