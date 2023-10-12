#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';
import { testEthereumDeposits } from '../shared/ethereum_deposits';
import { runWithTimeout, observeBadEvents } from '../shared/utils';
import { testFundRedeem } from '../shared/fund_redeem';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';
import { testLpApi } from '../shared/lp_api_test';

async function runAllConcurrentTests() {
  let stopObserving = false;
  const observingBadEvents = observeBadEvents(':BroadcastAborted', () => stopObserving);

  await Promise.all([
    testAllSwaps(),
    testEthereumDeposits(),
    testFundRedeem('redeem'),
    testMultipleMembersGovernance(),
    testLpApi(),
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
