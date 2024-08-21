#!/usr/bin/env -S pnpm tsx
import { SwapContext, testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

const swapContext = new SwapContext();

async function main(): Promise<void> {
  await testAllSwaps(swapContext);
  swapContext.print_report();
}

runWithTimeout(main(), 3000000)
  .then(() => {
    // There are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    swapContext.print_report();
    const now = new Date();
    const timestamp = `${now.getHours()}:${now.getMinutes()}:${now.getSeconds()}`;
    console.error(`${timestamp} ${error}`);
    process.exit(-1);
  });
