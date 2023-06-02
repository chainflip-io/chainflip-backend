#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted in ETH
//
// For example: ./commands/fund_eth.sh 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 ETH to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

const Web3 = require('web3');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const eth_endpoint = process.env.ETH_ENDPOINT || "http://127.0.0.1:8545";
	const ethereum_address = process.argv[2];
	const eth_amount = process.argv[3].trim();
	var wei_amount;
	if(eth_amount.indexOf('.') == -1){
		wei_amount = eth_amount + "000000000000000000";
	} else {
		const amount_parts = eth_amount.split('.');
		wei_amount = amount_parts[0] + amount_parts[1].padEnd(18,'0').substr(0, 18);
	}
	const web3 = new Web3(eth_endpoint);
	const chainid = await web3.eth.getChainId();
	const whaleKey = process.env.ETH_USDC_WHALE || '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
	const tx = {to: ethereum_address,
				value: wei_amount,
				gas: 2000000};

	console.log("Transferring " + eth_amount + " ETH to " + ethereum_address);
	const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
	let receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction);
	process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
