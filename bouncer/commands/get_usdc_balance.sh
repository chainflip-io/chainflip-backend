#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Usdc balance of the address provided as the first argument.
//
// For example: ./commands/get_usdc_balance.sh 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 
// might print: 100.2

const Web3 = require('web3');
const { runWithTimeout } = require('../shared/utils');

const erc20BalanceABI = [
  // balanceOf
  {
    "constant": true,
    "inputs": [
      {
        "name": "account",
        "type": "address"
      }
    ],
    "name": "balanceOf",
    "outputs": [
      {
        "name": "balance",
        "type": "uint256"
      }
    ],
    "type": "function"
  }
];

async function main() {
	const eth_endpoint = process.env.ETH_ENDPOINT || "http://127.0.0.1:8545";
	const ethereum_address = process.argv[2] || '0';
	const web3 = new Web3(eth_endpoint);
	const usdcContractAddress = process.env.ETH_USDC_ADDRESS || '0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512';
	const usdcContract = new web3.eth.Contract(erc20BalanceABI, usdcContractAddress);

	const raw_balance = await usdcContract.methods.balanceOf(ethereum_address).call();
	const balance_len = raw_balance.length;
	var balance;
	if(balance_len > 6){
		const decimal_location = balance_len - 6;
		balance = raw_balance.substr(0, decimal_location) + '.' + raw_balance.substr(decimal_location);
	} else {
		balance = "0." + raw_balance.padStart(6, '0');
	}
	console.log(balance);
	process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
