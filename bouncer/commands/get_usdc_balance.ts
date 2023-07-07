// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Usdc balance of the address provided as the first argument.
//
// For example: pnpm tsx ./commands/get_usdc_balance.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6
// might print: 100.2

import { runWithTimeout } from '../shared/utils';
import { getUsdcBalance } from '../shared/get_usdc_balance';

async function getUsdcBalanceCommand(ethereumAddress: string) {
  const balance = await getUsdcBalance(ethereumAddress);
  console.log(balance);
  process.exit(0);
}

const ethereumAddress = process.argv[2] ?? '0';

runWithTimeout(getUsdcBalanceCommand(ethereumAddress), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
