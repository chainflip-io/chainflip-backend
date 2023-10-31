#!/usr/bin/env -S pnpm tsx
import { performAndTrackSwap } from '../shared/perform_swap';

import { newAddress, runWithTimeout } from '../shared/utils';

export async function swapLessThanED() {
  console.log('=== Testing USDC -> DOT swaps obtaining less than ED ===');

  // The initial price is 10USDC = 1DOT,
  // we will swap only 5 USDC and check that the swap is completed successfully
  const tag = `USDC -> DOT (less than ED)`;
  const address = await newAddress('DOT', '!testing less than ED output for dot swaps!');

  console.log('Generated DOT address: ' + address);

  await performAndTrackSwap('USDC', 'DOT', address, '5', tag);

  console.log('=== Test complete ===');
}

runWithTimeout(swapLessThanED(), 500000)
  .then(() => {
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
