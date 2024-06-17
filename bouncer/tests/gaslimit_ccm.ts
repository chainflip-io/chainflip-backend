#!/usr/bin/env -S pnpm tsx
import { testGasLimitCcmSwaps } from '../shared/gaslimit_ccm';
import { runWithTimeout } from '../shared/utils';
import { observeBadEvent } from '../shared/utils/substrate';

// Running this test separately from all the concurrent tests because there will
// be BroadcastAborted events emited.
async function testGasLimitCcmTest() {
  console.log('=== Testing GasLimit CCM swaps ===');

  const feeDeficitRefused = observeBadEvent(':TransactionFeeDeficitRefused', {});

  await testGasLimitCcmSwaps();

  await feeDeficitRefused.stop();

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
