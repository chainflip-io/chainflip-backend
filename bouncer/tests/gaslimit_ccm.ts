#!/usr/bin/env -S pnpm tsx
import { testGasLimitCcmSwaps } from '../shared/gaslimit_ccm';
import { executeWithTimeout } from '../shared/utils';
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

await executeWithTimeout(testGasLimitCcmTest(), 1800);
