#!/usr/bin/env -S pnpm tsx
import { getDotBalance } from './get_dot_balance';
import { performAndTrackSwap } from './perform_swap';
import { getSwapRate, newAddress } from './utils';

const DOT_EXISTENTIAL_DEPOSIT = 1;

export async function swapLessThanED() {
  console.log('=== Testing USDC -> DOT swaps obtaining less than ED ===');
  const tag = `USDC -> DOT (less than ED)`;

  // we will try to swap with 5 USDC and check if the expected output is low enough
  // otherwise we'll keep reducing the amount
  let retry = true;
  let inputAmount = '5';
  while (retry) {
    let outputAmount = await getSwapRate('USDC', 'DOT', inputAmount);

    while (parseFloat(outputAmount) >= DOT_EXISTENTIAL_DEPOSIT) {
      inputAmount = (parseFloat(inputAmount) / 2).toString();
      outputAmount = await getSwapRate('USDC', 'DOT', inputAmount);
    }
    console.log(`${tag} Input amount: ${inputAmount} USDC`);
    console.log(`${tag} Approximate expected output amount: ${outputAmount} DOT`);

    // we want to be sure to have an address with 0 balance, hence we create a new one every time
    const address = await newAddress(
      'DOT',
      '!testing less than ED output for dot swaps!' + inputAmount + outputAmount,
    );
    console.log(`${tag} Generated DOT address: ${address}`);

    await performAndTrackSwap('USDC', 'DOT', address, inputAmount, tag);
    // if for some reason the balance after swapping is > 0 it means that the output was larger than
    // ED, so we'll retry the test with a lower input
    if (parseFloat(await getDotBalance(address)) > 0) {
      console.log(`${tag}, swap output was more than ED, retrying with less...`);
      inputAmount = (parseFloat(inputAmount) / 3).toString();
    } else {
      retry = false;
    }
  }
  console.log('=== Test USDC -> DOT swaps obtaining less than ED complete ===');
}
