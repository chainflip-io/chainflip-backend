#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Usdc balance of the address provided as the first argument.
//
// For example: ./commands/get_arbusdc_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 100.2

import { runWithTimeoutAndExit, getContractAddress } from '../shared/utils';
import { getErc20Balance } from '../shared/get_erc20_balance';

async function getUsdcBalanceCommand(arbitrumAddress: string) {
  const contractAddress = getContractAddress('Arbitrum', 'ArbUsdc');
  console.log(await getErc20Balance('Arbitrum', arbitrumAddress, contractAddress));
}

const arbitrumAddress = process.argv[2] ?? '0';
await runWithTimeoutAndExit(getUsdcBalanceCommand(arbitrumAddress), 5);
