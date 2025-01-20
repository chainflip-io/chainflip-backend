#!/usr/bin/env -S pnpm tsx
//
// Usage:
// run_test.ts <test_name> <optional_arg1> <optional_arg2>
//
// To get a list of available tests:
// ./commands/run_test.ts -h
//
//  Examples:
// ./commands/run_test.ts ./tests/fill_or_kill.ts
// ./commands/run_test.ts "Fund/Redeem" "my_seed"
// ./commands/run_test.ts DCA-Swaps
// ./commands/run_test.ts "dca swaps"
//
// Notes:
// You can provide the test name as the test file name or file path.
// You can instead provide the tests given name. It is not case sensitive and can be provided with or without spaces and hyphens.
// Only some test use the optional arguments. If one is provided and not used, it will be ignored. See the test file for details.
// If your new test is not on the list, add it to the allTests array.

import { testBrokerFeeCollection } from '../tests/broker_fee_collection';
import { testBtcUtxoConsolidation } from '../tests/btc_utxo_consolidation';
import { testEvmDeposits } from '../tests/evm_deposits';
import { testFillOrKill } from '../tests/fill_or_kill';
import { testFundRedeem } from '../tests/fund_redeem';
import { testLpApi } from '../tests/lp_api_test';
import { testMultipleMembersGovernance } from '../tests/multiple_members_governance';
import { testDoubleDeposit } from '../tests/double_deposit';
import { testMinimumDeposit } from '../tests/minimum_deposit';
import { testPolkadotRuntimeUpdate } from '../tests/polkadot_runtime_update';
import { testRotatesThroughBtcSwap } from '../tests/rotates_through_btc_swap';
import { testRotateAndSwap } from '../tests/rotation_barrier';
import { testGasLimitCcmSwaps } from '../tests/gaslimit_ccm';
import { testSwapAfterDisconnection } from '../tests/swap_after_temp_disconnecting_chains';
import { swapLessThanED } from '../tests/swap_less_than_existential_deposit_dot';
import { testAllSwaps } from '../tests/all_swaps';
import { testBoostingSwap } from '../tests/boost';
import { ConsoleColors, ConsoleLogColors } from '../shared/utils';
import { testDeltaBasedIngress } from '../tests/delta_based_ingress';
import { testCancelOrdersBatch } from '../tests/create_and_delete_multiple_orders';
import { depositChannelCreation } from '../tests/request_swap_deposit_address_with_affiliates';
import { testDCASwaps } from '../tests/DCA_test';
import { testVaultSwapFeeCollection } from '../tests/vault_swap_fee_collection';
import { testBrokerLevelScreening } from '../tests/broker_level_screening';
import { testSolanaVaultSettingsGovernance } from '../tests/solana_vault_settings_governance';

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
    testDeltaBasedIngress,
    testCancelOrdersBatch,
    depositChannelCreation,
    testVaultSwapFeeCollection,
    testBrokerLevelScreening,
    testSolanaVaultSettingsGovernance,
  ];

  // Help message
  if (testName === undefined || testName === '-h' || testName === '--help') {
    console.log('Usage: run_test.ts <test_name_or_file_path> <optional_arg1> <optional_arg2> ...');
    console.log('Available tests:');
    for (const test of allTests) {
      console.log(
        ConsoleLogColors.LightBlue,
        `  ${test.fileName} ${ConsoleColors.Dim}${test.name}${ConsoleColors.Reset}`,
      );
    }
    process.exit(0);
  }

  // Match the test and run it
  const additionalArgs = process.argv.slice(3);
  for (const test of allTests) {
    if (
      testName.toLowerCase().replace(/-/g, '').replace(/ /g, '') ===
        test.name.toLowerCase().replace(/-/g, '').replace(/ /g, '') ||
      testName.includes(test.filePath) ||
      testName.includes(test.fileName.replace('.ts', ''))
    ) {
      // This will exit the process when the test is done.
      await test.runAndExit(...additionalArgs);
    }
  }

  console.error(`Test "${testName}" not found. Use -h for a list of available tests.`);
  process.exit(-1);
}

await main();
