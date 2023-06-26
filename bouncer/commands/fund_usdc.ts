// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the ethereum address provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted as USDC
//
// For example: pnpm tsx ./commands/fund_usdc.ts 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6 1.2
// will send 1.2 USDC to account 0xcf1dc766fc2c62bef0b67a8de666c8e67acf35f6

import { runWithTimeout } from '../shared/utils';
import { fundUsdc } from '../shared/fund_usdc';

async function main(): Promise<void> {
  const ethereumAddress = process.argv[2];
  const usdcAmount = process.argv[3].trim();

  console.log("Submitting transaction to transfer USDC to " + ethereumAddress + " for " + usdcAmount + " USDC")

  await fundUsdc(ethereumAddress, usdcAmount);

  process.exit(0);
}

runWithTimeout(main(), 20000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
