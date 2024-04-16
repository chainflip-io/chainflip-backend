#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the ArbEth balance of the address provided as the first argument.
//
// For example: ./commands/get_arbeth_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 1.2

import { runWithTimeout } from '../shared/utils';
import { getEvmNativeBalance } from '../shared/get_evm_native_balance';

export async function getArbBalanceCommand(address: string) {
  const balance = await getEvmNativeBalance('Arbitrum', address);
  console.log(balance);
  process.exit(0);
}

const arbitrumAddress = process.argv[2] ?? '0';

runWithTimeout(getArbBalanceCommand(arbitrumAddress), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
