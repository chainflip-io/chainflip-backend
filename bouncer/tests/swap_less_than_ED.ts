#!/usr/bin/env -S pnpm tsx
import { swapLessThanED } from '../shared/swap_less_than_existential_deposit_dot';
import { runWithTimeout } from '../shared/utils';

async function main() {
  await swapLessThanED();
  process.exit(0);
}

runWithTimeout(main(), 300000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
