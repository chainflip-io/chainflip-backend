import { describe } from 'vitest';
import { testBoostingSwap } from './boost';
import { testVaultSwapFeeCollection } from './vault_swap_fee_collection';
import { testPolkadotRuntimeUpdate } from './polkadot_runtime_update';
import { checkSolEventAccountsClosure } from '../shared/sol_vault_swap';
import { checkAvailabilityAllSolanaNonces } from '../shared/utils';
import { swapLessThanED } from './swap_less_than_existential_deposit_dot';
import { testAllSwaps } from './all_swaps';
import { testEvmDeposits } from './evm_deposits';
import { testMultipleMembersGovernance } from './multiple_members_governance';
import { testLpApi } from './lp_api_test';
import { testBrokerFeeCollection } from './broker_fee_collection';
import { testFillOrKill } from './fill_or_kill';
import { testDCASwaps } from './DCA_test';
import { testCancelOrdersBatch } from './create_and_delete_multiple_orders';
import { depositChannelCreation } from './request_swap_deposit_address_with_affiliates';
import { testBrokerLevelScreening } from './broker_level_screening';
import { legacyEvmVaultSwaps } from './legacy_vault_swap';
import { testFundRedeem } from './fund_redeem';
import { concurrentTest, serialTest } from '../shared/utils/vitest';

// Tests that will run in parallel by both the ci-development and the ci-main-merge
describe('ConcurrentTests', () => {
  // Specify the number of nodes via setting the env var.
  // NODE_COUNT="3-node" pnpm vitest run -t "ConcurrentTests"
  const match = process.env.NODE_COUNT ? process.env.NODE_COUNT.match(/\d+/) : null;
  const givenNumberOfNodes = match ? parseInt(match[0]) : null;
  const numberOfNodes = givenNumberOfNodes ?? 1;

  concurrentTest('SwapLessThanED', swapLessThanED, 300);
  concurrentTest('AllSwaps', testAllSwaps, numberOfNodes === 1 ? 1200 : 1800); // TODO: find out what the 3-node timeout should be
  concurrentTest('EvmDeposits', testEvmDeposits, 250);
  concurrentTest('FundRedeem', testFundRedeem, 1000);
  concurrentTest('MultipleMembersGovernance', testMultipleMembersGovernance, 120);
  concurrentTest('LpApi', testLpApi, 200);
  concurrentTest('BrokerFeeCollection', testBrokerFeeCollection, 200);
  concurrentTest('BoostingForAsset', testBoostingSwap, 120);
  concurrentTest('FillOrKill', testFillOrKill, 600);
  concurrentTest('DCASwaps', testDCASwaps, 150);
  concurrentTest('CancelOrdersBatch', testCancelOrdersBatch, 240);
  concurrentTest('DepositChannelCreation', depositChannelCreation, 360);
  concurrentTest('BrokerLevelScreening', testBrokerLevelScreening, 300);
  concurrentTest('legacyEvmVaultSwaps', legacyEvmVaultSwaps, 300);
  concurrentTest('VaultSwapFeeCollection', testVaultSwapFeeCollection, 600);

  // Tests that only work if there is more than one node
  if (numberOfNodes > 1) {
    concurrentTest('PolkadotRuntimeUpdate', testPolkadotRuntimeUpdate, 1300);
  }

  // Post test checks
  serialTest('CheckSolEventAccountsClosure', checkSolEventAccountsClosure, 150);
  serialTest('CheckAvailabilityAllSolanaNonces', checkAvailabilityAllSolanaNonces, 50);
});
