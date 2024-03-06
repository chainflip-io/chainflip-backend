#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';
import { testEvmDeposits } from '../shared/evm_deposits';
import { runWithTimeout, observeBadEvents } from '../shared/utils';
import { testFundRedeem } from '../shared/fund_redeem';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';
import { testLpApi } from '../shared/lp_api_test';
import { swapLessThanED } from '../shared/swap_less_than_existential_deposit_dot';
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';

async function runAllConcurrentTests() {
  // Specify the number of nodes via providing an argument to this script.
  // Using regex because the localnet script passes in "3-node".
  const match = process.argv[2] ? process.argv[2].match(/\d+/) : null;
  const givenNumberOfNodes = match ? parseInt(match[0]) : null;
  const numberOfNodes = givenNumberOfNodes ?? 1;

  let stopObserving = false;
  const broadcastAborted = observeBadEvents(':BroadcastAborted', () => stopObserving);
  const feeDeficitRefused = observeBadEvents(':TransactionFeeDeficitRefused', () => stopObserving);

  // Tests that work with any number of nodes
  const tests = [
    swapLessThanED(),
    testAllSwaps(),
    testEvmDeposits(),
    testFundRedeem('redeem'),
    testMultipleMembersGovernance(),
    testLpApi(),
  ];

  // Test that only work if there is more than one node
  if (numberOfNodes > 1) {
    console.log(`Also running multi-node tests (${numberOfNodes} nodes)`);
    const multiNodeTests = [testPolkadotRuntimeUpdate()];
    tests.push(...multiNodeTests);
  }

  await Promise.all([...tests]);

  // Gracefully exit the broadcast abort observer
  stopObserving = true;
  await Promise.all([broadcastAborted, feeDeficitRefused]);
}

runWithTimeout(runAllConcurrentTests(), 1000000)
  .then(() => {
    // There are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
