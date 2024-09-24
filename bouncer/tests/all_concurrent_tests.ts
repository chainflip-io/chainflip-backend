#!/usr/bin/env -S NODE_OPTIONS=--max-old-space-size=6144 pnpm tsx
import { testEvmDeposits } from './evm_deposits';
import { checkAvailabilityAllSolanaNonces, runWithTimeoutAndExit } from '../shared/utils';
import { testFundRedeem } from './fund_redeem';
import { testMultipleMembersGovernance } from './multiple_members_governance';
import { testLpApi } from './lp_api_test';
import { swapLessThanED } from './swap_less_than_existential_deposit_dot';
import { testPolkadotRuntimeUpdate } from './polkadot_runtime_update';
import { testBrokerFeeCollection } from './broker_fee_collection';
import { testBoostingSwap } from './boost';
import { observeBadEvent } from '../shared/utils/substrate';
import { testFillOrKill } from './fill_or_kill';
import { testDCASwaps } from './DCA_test';
import { testCancelOrdersBatch } from './create_and_delete_multiple_orders';
import { testAllSwaps } from './all_swaps';

async function runAllConcurrentTests() {
  // Specify the number of nodes via providing an argument to this script.
  // Using regex because the localnet script passes in "3-node".
  const match = process.argv[2] ? process.argv[2].match(/\d+/) : null;
  const givenNumberOfNodes = match ? parseInt(match[0]) : null;
  const numberOfNodes = givenNumberOfNodes ?? 1;

  const broadcastAborted = observeBadEvent(':BroadcastAborted', {
    label: 'Concurrent broadcast aborted',
  });
  const feeDeficitRefused = observeBadEvent(':TransactionFeeDeficitRefused', {
    label: 'Concurrent fee deficit refused',
  });

  // Tests that work with any number of nodes and can be run concurrently
  const tests = [
    swapLessThanED.run(),
    testAllSwaps.run(),
    testEvmDeposits.run(numberOfNodes),
    testFundRedeem.run('redeem'),
    testMultipleMembersGovernance.run(),
    testLpApi.run(),
    testBrokerFeeCollection.run(),
    testBoostingSwap.run(),
    testFillOrKill.run(),
    testDCASwaps.run(),
    testCancelOrdersBatch.run(),
  ];

  // Tests that only work if there is more than one node
  if (numberOfNodes > 1) {
    console.log(`Also running multi-node tests (${numberOfNodes} nodes)`);
    const multiNodeTests = [testPolkadotRuntimeUpdate.run()];
    tests.push(...multiNodeTests);
  }

  await Promise.all(tests);

  await Promise.all([broadcastAborted.stop(), feeDeficitRefused.stop()]);

  await checkAvailabilityAllSolanaNonces();
}

await runWithTimeoutAndExit(runAllConcurrentTests(), 2000);
