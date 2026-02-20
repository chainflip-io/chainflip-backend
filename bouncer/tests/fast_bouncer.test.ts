import { describe } from 'vitest';
import { testBoostingSwap } from 'tests/boost';
import { testVaultSwap } from 'tests/vault_swap_tests';
import { testPolkadotRuntimeUpdate } from 'tests/polkadot_runtime_update';
import { checkSolEventAccountsClosure } from 'shared/sol_vault_swap';
import { checkAvailabilityAllSolanaNonces } from 'shared/utils';
import { swapLessThanED } from 'tests/swap_less_than_existential_deposit_dot';
import { testAllSwaps, testSwapsToAssethub } from 'tests/all_swaps';
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
import { testCcmSwapFundAccount, testDelegate } from 'tests/delegate_flip';
import { testSpecialBitcoinSwaps } from 'tests/special_btc_swaps';
import { testSignedRuntimeCall } from 'tests/signed_runtime_call';
import { lendingTest } from 'tests/lending';
import { testGovernanceDepositWitnessing } from 'tests/governance_deposit_witnessing';

// Tests that will run in parallel by both the ci-development and the ci-main-merge
describe('ConcurrentTests', () => {
  // Specify the number of nodes via setting the env var.
  // NODE_COUNT="3-node" pnpm vitest --maxConcurrency=100 run -t "ConcurrentTests"
  const match = process.env.NODE_COUNT ? process.env.NODE_COUNT.match(/\d+/) : null;
  const numberOfNodes = match ? parseInt(match[0]) : 1;
  const singleSwapTimeout = numberOfNodes === 1 ? 260 : 300;
  const ciTimeoutFactor = 1.4; // Adjustment factor for CI, since all timeouts are set by running on Mac M4

  testAllSwaps(singleSwapTimeout * ciTimeoutFactor);
  concurrentTest('SwapsToAssethub', testSwapsToAssethub, 300 * ciTimeoutFactor);
  concurrentTest('EvmDeposits', testEvmDeposits, 310 * ciTimeoutFactor);
  concurrentTest('FundRedeem', testFundRedeem, 350 * ciTimeoutFactor);
  concurrentTest('BoostingForAsset', testBoostingSwap, 340 * ciTimeoutFactor);
  concurrentTest('LpApi', testLpApi, 265 * ciTimeoutFactor);
  concurrentTest('BrokerFeeCollection', testBrokerFeeCollection, 200 * ciTimeoutFactor);
  concurrentTest('FillOrKill', testFillOrKill, 280 * ciTimeoutFactor);
  concurrentTest('DCASwaps', testDCASwaps, 190 * ciTimeoutFactor);
  concurrentTest('CancelOrdersBatch', testCancelOrdersBatch, 240 * ciTimeoutFactor);
  concurrentTest('DepositChannelCreation', depositChannelCreation, 50 * ciTimeoutFactor);
  concurrentTest('BrokerLevelScreening', testBrokerLevelScreening, 340 * ciTimeoutFactor);
  concurrentTest('VaultSwaps', testVaultSwap, 330 * ciTimeoutFactor);
  concurrentTest('SpecialBitcoinSwaps', testSpecialBitcoinSwaps, 200 * ciTimeoutFactor);
  concurrentTest('DelegateFlip', testDelegate, 325 * ciTimeoutFactor);
  concurrentTest('SwapAndFundAccountViaCCM', testCcmSwapFundAccount, 240 * ciTimeoutFactor);
  concurrentTest('SignedRuntimeCall', testSignedRuntimeCall, 240 * ciTimeoutFactor);
  concurrentTest('Lending', lendingTest, 348 * ciTimeoutFactor);
  concurrentTest(
    'GovernanceDepositWitnessing',
    testGovernanceDepositWitnessing,
    265 * ciTimeoutFactor,
  );

  // Test this separately because it has a swap to HubDot which causes flakiness when run in
  // parallel with the Assethub tests in `SwapsToAssethub`.
  // serialTest('SwapLessThanED', swapLessThanED, 350 * ciTimeoutFactor);

  // Test this separately since some other tests rely on single member governance.
  serialTest('MultipleMembersGovernance', testMultipleMembersGovernance, 60 * ciTimeoutFactor);

  // Tests that only work if there is more than one node
  if (numberOfNodes > 1) {
    concurrentTest('PolkadotRuntimeUpdate', testPolkadotRuntimeUpdate, 1300 * ciTimeoutFactor);
  }

  // Post test checks
  serialTest('CheckSolEventAccountsClosure', checkSolEventAccountsClosure, 5 * ciTimeoutFactor);
  serialTest(
    'CheckAvailabilityAllSolanaNonces',
    checkAvailabilityAllSolanaNonces,
    5 * ciTimeoutFactor,
  );
});

// Run only the broker level screening tests
describe('BrokerLevelScreeningTestWithBoost', () => {
  concurrentTest('BrokerLevelScreening', (context) => testBrokerLevelScreening(context, true), 600);
});

describe('AllSwaps', () => {
  const match = process.env.NODE_COUNT ? process.env.NODE_COUNT.match(/\d+/) : null;
  const numberOfNodes = match ? parseInt(match[0]) : 1;

  testAllSwaps(numberOfNodes === 1 ? 180 : 240); // Adjust timeout based on node count
});
