#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Eth balance of the address provided as the first argument.
//
// For example: ./commands/get_eth_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 1.2

import { executeWithTimeout } from '../shared/utils';
import { getEvmNativeBalance } from '../shared/get_evm_native_balance';

export async function getEthBalanceCommand(address: string) {
  console.log(await getEvmNativeBalance('Ethereum', address));
}

const ethereumAddress = process.argv[2] ?? '0';
await executeWithTimeout(getEthBalanceCommand(ethereumAddress), 5);
