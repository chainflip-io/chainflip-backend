#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new ethereum address and return the address
// For example: ./commands/new_eth_address.sh foobar
// returns: 0xE16CCFc63368e8FC93f53ccE4e4f4b08c4C3E186

const Web3 = require('web3');
const sha256 = require('sha256');

function main() {
	const seed = process.argv[2] || '';
	const secret = sha256(seed);
	const web3 = new Web3();
	console.log(web3.eth.accounts.privateKeyToAccount(secret).address);
}

main();