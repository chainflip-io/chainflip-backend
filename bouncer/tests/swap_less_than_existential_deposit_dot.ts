#!/usr/bin/env -S pnpm tsx
import { getDotBalance } from '../shared/get_dot_balance';
import { performAndTrackSwap } from '../shared/perform_swap';

import { getSwapRate, newAddress, runWithTimeout } from '../shared/utils';

export async function swapLessThanED() {
  console.log('=== Testing USDC -> DOT swaps obtaining less than ED ===');
  const tag = `USDC -> DOT (less than ED)`;
  // The initial price is 10USDC = 1DOT,
  // we will swap only 5 USDC and check that the swap is completed successfully with 0 output
  let retry = true;
  let inputAmount = '5';
  while (retry) {
    let outputAmount = await getSwapRate('USDC', 'DOT', inputAmount);

    while (parseFloat(outputAmount) >= 1) {
      inputAmount = (parseFloat(inputAmount) / 2).toString();
      outputAmount = await getSwapRate('USDC', 'DOT', inputAmount);
    }
    console.log(`Input amount: ${inputAmount} USDC`);
    console.log(`Output amount: ${outputAmount} DOT`);

    // we want to be sure to have an address with 0 balance, hence we create a new one every time
    const address = await newAddress(
      'DOT',
      '!testing less than ED output for dot swaps!' + inputAmount + outputAmount,
    );
    console.log('Generated DOT address: ' + address);

    await performAndTrackSwap('USDC', 'DOT', address, inputAmount, tag);
    if (parseFloat(await getDotBalance(address)) > 0) {
      console.log(`${tag}, swap output was more than ED, retrying with less...`);
      inputAmount = (parseFloat(inputAmount) / 3).toString();
    } else {
      retry = false;
    }
  }
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
