#!/usr/bin/env -S NODE_OPTIONS=--max-old-space-size=6144 pnpm tsx
import { SwapContext, testAllSwaps } from '../shared/swapping';
import { testEvmDeposits } from '../shared/evm_deposits';
import { runWithTimeout } from '../shared/utils';
import { testFundRedeem } from '../shared/fund_redeem';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';
import { testLpApi } from '../shared/lp_api_test';
import { swapLessThanED } from '../shared/swap_less_than_existential_deposit_dot';
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';
import { testBrokerFeeCollection } from '../shared/broker_fee_collection';
import { testBoostingSwap } from '../shared/boost';
import { observeBadEvent } from '../shared/utils/substrate';

const swapContext = new SwapContext();

async function runAllConcurrentTests() {
  // Specify the number of nodes via providing an argument to this script.
  // Using regex because the localnet script passes in "3-node".
  const match = process.argv[2] ? process.argv[2].match(/\d+/) : null;
  const givenNumberOfNodes = match ? parseInt(match[0]) : null;
  const numberOfNodes = givenNumberOfNodes ?? 1;

  const broadcastAborted = observeBadEvent(
    ':BroadcastAborted',
    { label: 'Concurrent broadcast aborted' }
  );
  const feeDeficitRefused = observeBadEvent(
    ':TransactionFeeDeficitRefused',
    {
      label: 'Concurrent fee deficit refused',
    }
  );

  // Tests that work with any number of nodes and can be run concurrently
  const tests = [
    swapLessThanED(),
    testAllSwaps(swapContext),
    testEvmDeposits(),
    testFundRedeem('redeem'),
    testMultipleMembersGovernance(),
    testLpApi(),
    testBrokerFeeCollection(),
    testBoostingSwap(),
  ];

  // Test that only work if there is more than one node
  if (numberOfNodes > 1) {
    console.log(`Also running multi-node tests (${numberOfNodes} nodes)`);
    const multiNodeTests = [testPolkadotRuntimeUpdate()];
    tests.push(...multiNodeTests);
  }

  await Promise.all([...tests]);

  await Promise.all([broadcastAborted.stop(), feeDeficitRefused.stop()]);
}

runWithTimeout(runAllConcurrentTests(), 2000000)
  .then(() => {
    // There are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error!('All concurrent tests timed out. Exiting.');
    swapContext.print_report();
    console.error(error);
    process.exit(-1);
  });
