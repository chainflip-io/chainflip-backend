#!/usr/bin/env -S pnpm tsx
import { testLpDepositExpiry } from '../shared/lp_deposit_expiry';
import { testAllSwaps } from '../shared/swapping';
import { testEthereumDeposits } from '../shared/ethereum_deposits';
import { testGasLimitCcmSwaps } from '../shared/ccm_gaslimit';
import { runWithTimeout, observeBadEvents } from '../shared/utils';

async function runAllConcurrentTests() {
  let stopObserving = false;
  const observingBadEvents = observeBadEvents(':BroadcastAborted', () => stopObserving);

  await Promise.all([
    testAllSwaps(),
    testLpDepositExpiry(),
    testEthereumDeposits(),
    testGasLimitCcmSwaps(),
  ]);

  // Gracefully exit the broadcast abort observer
  stopObserving = true;
  await observingBadEvents;
}

runWithTimeout(runAllConcurrentTests(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
