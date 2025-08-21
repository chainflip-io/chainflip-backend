import { describe } from 'vitest';
import { testBoostingSwap } from 'tests/boost';
import { testVaultSwap } from 'tests/vault_swap_tests';
import { testPolkadotRuntimeUpdate } from 'tests/polkadot_runtime_update';
import { checkSolEventAccountsClosure } from 'shared/sol_vault_swap';
import { checkAvailabilityAllSolanaNonces } from 'shared/utils';
import { swapLessThanED } from 'tests/swap_less_than_existential_deposit_dot';
import { testAllSwaps } from 'tests/all_swaps';
import { testEvmDeposits } from 'tests/evm_deposits';
import { testMultipleMembersGovernance } from 'tests/multiple_members_governance';
import { testLpApi } from 'tests/lp_api_test';
import { testBrokerFeeCollection } from 'tests/broker_fee_collection';
import { testFillOrKill } from 'tests/fill_or_kill';
import { testDCASwaps } from 'tests/DCA_test';
import { testCancelOrdersBatch } from 'tests/create_and_delete_multiple_orders';
import { depositChannelCreation } from 'tests/request_swap_deposit_address_with_affiliates';
import { testBrokerLevelScreening } from 'tests/broker_level_screening';
import { testFundRedeem } from 'tests/fund_redeem';
import { concurrentTest, serialTest } from 'shared/utils/vitest';
import { testAssethubXcm } from 'tests/assethub_xcm';
import { testDelegateFlip } from './delegate_flip';

// Tests that will run in parallel by both the ci-development and the ci-main-merge
describe('ConcurrentTests', () => {
  // Specify the number of nodes via setting the env var.
  // NODE_COUNT="3-node" pnpm vitest --maxConcurrency=100 run -t "ConcurrentTests"
  const match = process.env.NODE_COUNT ? process.env.NODE_COUNT.match(/\d+/) : null;
  const givenNumberOfNodes = match ? parseInt(match[0]) : null;
  const numberOfNodes = givenNumberOfNodes ?? 1;

  concurrentTest('SwapLessThanED', swapLessThanED, 180);
  testAllSwaps(numberOfNodes === 1 ? 180 : 240); // TODO: find out what the 3-node timeout should be
  concurrentTest('EvmDeposits', testEvmDeposits, 300);
  concurrentTest('FundRedeem', testFundRedeem, 600);
  concurrentTest('MultipleMembersGovernance', testMultipleMembersGovernance, 120);
  concurrentTest('LpApi', testLpApi, 240);
  concurrentTest('BrokerFeeCollection', testBrokerFeeCollection, 200);
  concurrentTest('BoostingForAsset', testBoostingSwap, 200);
  concurrentTest('FillOrKill', testFillOrKill, 300);
  concurrentTest('DCASwaps', testDCASwaps, 300);
  concurrentTest('CancelOrdersBatch', testCancelOrdersBatch, 120);
  concurrentTest('DepositChannelCreation', depositChannelCreation, 30);
  concurrentTest('BrokerLevelScreening', testBrokerLevelScreening, 600);
  concurrentTest('VaultSwaps', testVaultSwap, 600);
  concurrentTest('AssethubXCM', testAssethubXcm, 200);
  concurrentTest('DelegateFlip', testDelegateFlip, 360);

  // Tests that only work if there is more than one node
  if (numberOfNodes > 1) {
    concurrentTest('PolkadotRuntimeUpdate', testPolkadotRuntimeUpdate, 1300);
  }

  // Post test checks
  serialTest('CheckSolEventAccountsClosure', checkSolEventAccountsClosure, 150);
  serialTest('CheckAvailabilityAllSolanaNonces', checkAvailabilityAllSolanaNonces, 50);
});

// Run only the broker level screening tests
describe('BrokerLevelScreeningTest', () => {
  concurrentTest('BrokerLevelScreening', (context) => testBrokerLevelScreening(context, true), 600);
});

describe('AllSwaps', () => {
  testAllSwaps(240);
});
