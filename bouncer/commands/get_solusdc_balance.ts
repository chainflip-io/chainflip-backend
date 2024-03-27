#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Arb balance of the ATA of the address provided as the first argument.
//
// For example: ./commands/get_solusdc_balance.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE
// might print: 1.2
// It also accepts non-encoded bs58 address representations:
// ./commands/get_solusdc_balance.ts 0x2f3fcadf740018f6037513959bab60d0dbef26888d264d54fc4d3d36c8cf5c91

import { runWithTimeout } from '../shared/utils';
import { getSolUsdcBalance } from '../shared/get_solusdc_balance';

export async function getSolUsdcBalanceCommand(address: string) {
  const balance = await getSolUsdcBalance(address);
  console.log(balance);
  process.exit(0);
}

const solAddress = process.argv[2] ?? '0';

runWithTimeout(getSolUsdcBalanceCommand(solAddress), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
