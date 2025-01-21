#!/usr/bin/env -S pnpm tsx
import { randomBytes } from 'crypto';
import { newAddress } from '../shared/utils';
import { ExecutableTest } from '../shared/executable_test';
import { requestNewSwap } from '../shared/perform_swap';
import { sendSol } from '../shared/send_sol';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const spamSolanaDepositChannels = new ExecutableTest(
  'Spam-Solana-Deposit-Channels',
  main,
  1300,
);

async function requestBatchOfSwaps(batchSize: number): Promise<string[]> {
  const inputAsset = 'Sol';
  const destAsset = 'Flip';
  const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));
  const swapRequests = [];

  for (let i = 0; i < batchSize; i++) {
    const swapRequest = requestNewSwap(
      inputAsset,
      destAsset,
      destAddress,
      'Spam-Solana-Deposit-Channels',
      undefined,
      0,
      false,
      0,
    );

    swapRequests.push(swapRequest);
  }

  const swapParameters = await Promise.all(swapRequests);

  return swapParameters.map((val) => val.depositAddress);
}

// Opens 1000 deposit channels
// Execute: ./commands/run_test.ts spam_solana_deposit_channels.ts
async function main() {
  const batches = 200;
  const batchSize = 10;
  const solAmount = '0.001';

  for (let i = 1; i <= batches; i++) {
    console.log(`Opening batch ${i} of ${batches}`);
    const depositAddresses = await requestBatchOfSwaps(batchSize);
    console.log(`Sent funds to ${depositAddresses.length} deposit channels`);
    for (const depositAddress of depositAddresses) {
      await sendSol(depositAddress, solAmount);
      console.log(`Sent ${solAmount} Sol to ${depositAddress}`);
    }
  }
}
