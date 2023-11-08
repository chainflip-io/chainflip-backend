#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';
import { testEthereumDeposits } from '../shared/ethereum_deposits';
import { runWithTimeout, observeBadEvents } from '../shared/utils';
import { testFundRedeem } from '../shared/fund_redeem';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';
import { testLpApi } from '../shared/lp_api_test';
import { swapLessThanED } from '../shared/swap_less_than_existential_deposit_dot';

async function runAllConcurrentTests() {
  let stopObserving = false;
  const broadcastAborted = observeBadEvents(':BroadcastAborted', () => stopObserving);
  const feeDeficitRefused = observeBadEvents(':TransactionFeeDeficitRefused', () => stopObserving);

  await Promise.all([
    swapLessThanED(),
    testAllSwaps(),
    testEthereumDeposits(),
    testFundRedeem('redeem'),
    testMultipleMembersGovernance(),
    testLpApi(),
  ]);

  // Gracefully exit the broadcast abort observer
  stopObserving = true;
  await Promise.all([broadcastAborted, feeDeficitRefused]);
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
