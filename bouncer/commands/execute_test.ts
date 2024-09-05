#!/usr/bin/env -S pnpm tsx
//
// Usage:
// execute_test.ts <test_name> <optional_arg1> <optional_arg2>
//
// To get a list of available tests:
// ./commands/execute_test.ts -h
//
//  Examples:
// ./commands/execute_test.ts "Fund/Redeem" "my_seed"
// ./commands/execute_test.ts DCA-Swaps
// ./commands/execute_test.ts "dca swaps"
//
// Notes:
// The name of the test is not case sensitive and can be provided with or without spaces and hyphens.
// Only some test use the optional arguments. If one is provided and not used, it will be ignored. See the test file for details.
// If your new test is not on the list, add it to the allTests array.

import { testBoostingSwap } from '../tests/boost';
import { testBrokerFeeCollection } from '../tests/broker_fee_collection';
import { testBtcUtxoConsolidation } from '../tests/btc_utxo_consolidation';
import { testDCASwaps } from '../tests/DCA_test';
import { testEvmDeposits } from '../tests/evm_deposits';
import { testFillOrKill } from '../tests/fill_or_kill';
import { testFundRedeem } from '../tests/fund_redeem';
import { testLpApi } from '../tests/lp_api_test';
import { testMultipleMembersGovernance } from '../tests/multiple_members_governance';
import { swapLessThanED } from '../tests/swap_less_than_existential_deposit_dot';
import { testDoubleDeposit } from '../tests/double_deposit';
import { testMinimumDeposit } from '../tests/minimum_deposit';
import { testPolkadotRuntimeUpdate } from '../tests/polkadot_runtime_update';
import { testRotatesThroughBtcSwap } from '../tests/rotates_through_btc_swap';
import { testRotateAndSwap } from '../tests/rotation_barrier';
import { testGasLimitCcmSwaps } from '../tests/gaslimit_ccm';
import { testAllSwaps } from '../tests/all_swaps';
import { testSwapAfterDisconnection } from '../tests/swap_after_temp_disconnecting_chains';

async function main() {
  const testName = process.argv[2];

  // Every time we add a new test, we need to add it to this list.
  const allTests = [
    swapLessThanED,
    testAllSwaps,
    testEvmDeposits,
    testFundRedeem,
    testMultipleMembersGovernance,
    testLpApi,
    testBrokerFeeCollection,
    testBoostingSwap,
    testFillOrKill,
    testDCASwaps,
    testBtcUtxoConsolidation,
    testDoubleDeposit,
    testMinimumDeposit,
    testPolkadotRuntimeUpdate,
    testRotatesThroughBtcSwap,
    testRotateAndSwap,
    testGasLimitCcmSwaps,
    testSwapAfterDisconnection,
  ];

  // Help message
  if (testName === undefined || testName === '-h' || testName === '--help') {
    console.log('Usage: run_test.ts <test_name> <optional_arg1> <optional_arg2> ...');
    console.log('Available tests:');
    for (const test of allTests) {
      console.log(`\x1b[36m%s\x1b[0m`, `  ${test.name}`);
    }
    process.exit(0);
  }

  // Match the test and run it
  const additionalArgs = process.argv.slice(3);
  for (const test of allTests) {
    if (
      testName.toLowerCase().replace(/-/g, '').replace(/ /g, '') ===
      test.name.toLowerCase().replace(/-/g, '').replace(/ /g, '')
    ) {
      // This will exit the process when the test is done.
      await test.execute(...additionalArgs);
    }
  }

  console.error(`Test "${testName}" not found. Use -h for a list of available tests.`);
  process.exit(1);
}

await main();
