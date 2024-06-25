#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Usdc balance of the address provided as the first argument.
//
// For example: ./commands/get_usdc_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 100.2

import { executeWithTimeout, getContractAddress } from '../shared/utils';
import { getErc20Balance } from '../shared/get_erc20_balance';

async function getUsdcBalanceCommand(ethereumAddress: string) {
  const contractAddress = getContractAddress('Ethereum', 'Usdc');
  console.log(await getErc20Balance('Ethereum', ethereumAddress, contractAddress));
}

const ethereumAddress = process.argv[2] ?? '0';
await executeWithTimeout(getUsdcBalanceCommand(ethereumAddress), 5);
