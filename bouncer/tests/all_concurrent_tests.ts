#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';
import { testEvmDeposits } from '../shared/evm_deposits';
import { runWithTimeout, observeBadEvents } from '../shared/utils';
import { testFundRedeem } from '../shared/fund_redeem';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';
import { testLpApi } from '../shared/lp_api_test';
import { swapLessThanED } from '../shared/swap_less_than_existential_deposit_dot';
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';
import { testBrokerFeeCollection } from '../shared/broker_fee_collection';

async function runAllConcurrentTests() {
  // Specify the number of nodes via providing an argument to this script.
  // Using regex because the localnet script passes in "3-node".
  const numberOfNodes = Number.parseInt(process.argv[2]) || 1;

  const abortController = new AbortController();
  const broadcastAborted = observeBadEvents(':BroadcastAborted', abortController.signal);
  const feeDeficitRefused = observeBadEvents(
    ':TransactionFeeDeficitRefused',
    abortController.signal,
  );

  // Tests that work with any number of nodes
  const tests = [
    swapLessThanED().then(
      () => {
        console.log('swapLessThanED promise resolved');
      },
      () => {
        console.log('swapLessThanED promise rejected');
      },
    ),
    testAllSwaps().then(
      () => {
        console.log('testAllSwaps promise resolved');
      },
      () => {
        console.log('testAllSwaps promise rejected');
      },
    ),
    testEvmDeposits().then(
      () => {
        console.log('testEvmDeposits promise resolved');
      },
      () => {
        console.log('testEvmDeposits promise rejected');
      },
    ),
    testFundRedeem('redeem').then(
      () => {
        console.log('testFundRedeem promise resolved');
      },
      () => {
        console.log('testFundRedeem promise rejected');
      },
    ),
    testMultipleMembersGovernance().then(
      () => {
        console.log('testMultipleMembersGovernance promise resolved');
      },
      () => {
        console.log('testMultipleMembersGovernance promise rejected');
      },
    ),
    testLpApi().then(
      () => {
        console.log('testLpApi promise resolved');
      },
      () => {
        console.log('testLpApi promise rejected');
      },
    ),
    testBrokerFeeCollection().then(
      () => {
        console.log('testBrokerFeeCollection promise resolved');
      },
      () => {
        console.log('testBrokerFeeCollection promise rejected');
      },
    ),
  ];

  // Test that only work if there is more than one node
  if (numberOfNodes > 1) {
    console.log(`Also running multi-node tests (${numberOfNodes} nodes)`);
    const multiNodeTests = [testPolkadotRuntimeUpdate()];
    tests.push(...multiNodeTests);
  }

  await Promise.all(tests);

  // Gracefully exit the broadcast abort observer
  abortController.abort();
  await Promise.all([broadcastAborted, feeDeficitRefused]);
}

runWithTimeout(runAllConcurrentTests(), 2000000)
  .then(() => {
    // There are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error!('All concurrent tests timed out. Exiting.');
    console.error(error);
    process.exit(-1);
  });
