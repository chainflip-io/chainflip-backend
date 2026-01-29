#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Wbtc balance of the address provided as the first argument.
//
// For example: ./commands/get_wbtc_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 100.2

import { runWithTimeoutAndExit, getContractAddress } from 'shared/utils';
import { getErc20Balance } from 'shared/get_erc20_balance';

async function getWbtcBalanceCommand(ethereumAddress: string) {
  const contractAddress = getContractAddress('Ethereum', 'Wbtc');
  console.log(await getErc20Balance('Ethereum', ethereumAddress, contractAddress));
  process.exit(0);
}

const ethereumAddress = process.argv[2] ?? '0';
await runWithTimeoutAndExit(getWbtcBalanceCommand(ethereumAddress), 5);
