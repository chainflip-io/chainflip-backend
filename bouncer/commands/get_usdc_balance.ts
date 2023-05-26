#!/usr/bin/env pnpm tsx

// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Usdc balance of the address provided as the first argument.
//
// For example: ./commands/get_usdc_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 100.2

import Web3 from 'web3';
import { runWithTimeout } from '../shared/utils';

const erc20BalanceABI = [
  // balanceOf
  {
    constant: true,
    inputs: [
      {
        name: 'account',
        type: 'address',
      },
    ],
    name: 'balanceOf',
    outputs: [
      {
        name: 'balance',
        type: 'uint256',
      },
    ],
    type: 'function',
  },
];

async function main(): Promise<void> {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const ethereumAddress = process.argv[2] ?? '0';
  const web3 = new Web3(ethEndpoint);
  const usdcContractAddress =
    process.env.ETH_USDC_ADDRESS ?? '0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512';
  const usdcContract = new web3.eth.Contract(erc20BalanceABI as any, usdcContractAddress);

  const rawBalance: string = await usdcContract.methods.balanceOf(ethereumAddress).call();
  const balanceLen = rawBalance.length;
  let balance;
  if (balanceLen > 6) {
    const decimalLocation = balanceLen - 6;
    balance = rawBalance.slice(0, decimalLocation) + '.' + rawBalance.slice(decimalLocation);
  } else {
    balance = '0.' + rawBalance.padStart(6, '0');
  }
  console.log(balance);
  process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
