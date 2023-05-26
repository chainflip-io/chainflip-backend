#!/usr/bin/env pnpm tsx

// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Eth balance of the address provided as the first argument.
//
// For example: ./commands/get_eth_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 1.2

import Web3 from 'web3';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const ethereumAddress = process.argv[2] ?? '0';
  const web3 = new Web3(ethEndpoint);

  const weiBalance: string = await web3.eth.getBalance(ethereumAddress);
  const balanceLen = weiBalance.length;
  let balance;
  if (balanceLen > 18) {
    const decimalLocation = balanceLen - 18;
    balance = weiBalance.slice(0, decimalLocation) + '.' + weiBalance.slice(decimalLocation);
  } else {
    balance = '0.' + weiBalance.padStart(18, '0');
  }
  console.log(balance);
  process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
