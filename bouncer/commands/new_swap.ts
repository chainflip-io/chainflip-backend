// INSTRUCTIONS
//
// This command takes four arguments.
// It will request a new swap with the provided parameters
// Argument 1 is the source currency ("btc", "eth", "dot" or "usdc")
// Argument 2 is the destination currency ("btc", "eth", "dot" or "usdc")
// Argument 3 is the destination address
// Argument 4 is the broker fee in basis points
// For example: pnpm tsx ./commands/new_swap.ts dot btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX 100

import { Token, runWithTimeout } from '../shared/utils';
import { newSwap } from '../shared/new_swap';

async function newSwapCommand() {

  const sourceToken = process.argv[2].toUpperCase() as Token;
  const destToken = process.argv[3].toUpperCase() as Token;
  const destAddress = process.argv[4];
  const fee = process.argv[5];

  console.log(`Requesting swap ${sourceToken} -> ${destToken}`);

  await newSwap(sourceToken, destToken, destAddress, fee);

  process.exit(0);
}

runWithTimeout(newSwapCommand(), 60000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
