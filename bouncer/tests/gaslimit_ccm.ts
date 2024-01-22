#!/usr/bin/env -S pnpm tsx
import { testGasLimitCcmSwaps } from '../shared/gaslimit_ccm';
import {
  runWithTimeout,
  observeBadEvents,
  sleep,
  observeEvent,
  getChainflipApi,
} from '../shared/utils';

// Running this test separately from all the concurrent tests because there will
// be BroadcastAborted events emited.
async function testGasLimitCcmTest() {
  console.log('=== Testing GasLimit CCM swaps ===');

  let stopObserving = false;
  const feeDeficitRefused = observeBadEvents(':TransactionFeeDeficitRefused', () => stopObserving);

  await testGasLimitCcmSwaps();

  console.log('Waiting for the fee deficits to be recorded...');
  await observeEvent('ethereumBroadcaster:TransactionFeeDeficitRecorded', await getChainflipApi());
  // Wait for some blocks after the first fee deficit is recorded
  await sleep(30000);
  stopObserving = true;
  await feeDeficitRefused;

  console.log('=== GasLimit CCM test completed ===');
}

runWithTimeout(testGasLimitCcmTest(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
