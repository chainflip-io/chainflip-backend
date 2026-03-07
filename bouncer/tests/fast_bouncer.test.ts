import { describe } from 'vitest';
import { testBoostingSwap } from 'tests/boost';
import { testVaultSwap } from 'tests/vault_swap_tests';
import { testPolkadotRuntimeUpdate } from 'tests/polkadot_runtime_update';
import { checkSolEventAccountsClosure } from 'shared/sol_vault_swap';
import { checkAvailabilityAllSolanaNonces } from 'shared/utils';
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
  const inCi = !!process.env.GITHUB_ACTIONS;
  // CI runners are slower, use a larger timeout factor
  const ciTimeoutFactor = inCi ? 1.6 : 1.1;

  // Launch all tests in parallel. This will create a lot of contention for the first few blocks.
  // The concurrentTest function can be called with startDelaySeconds parameter that will delay the start of the
  // test to reduce contention, for example, the BrokerLevelScreeningTest is delayed to not end up
  // in situations where the deposit monitor is slow in flagging transactions.
  testAllSwaps(singleSwapTimeout * ciTimeoutFactor);
  concurrentTest('SwapsToAssethub', testSwapsToAssethub, 330 * ciTimeoutFactor);
  concurrentTest('EvmDeposits', testEvmDeposits, 280 * ciTimeoutFactor);
  concurrentTest('FundRedeem', testFundRedeem, 350 * ciTimeoutFactor);
  concurrentTest('LpApi', testLpApi, 280 * ciTimeoutFactor);
  concurrentTest('BrokerFeeCollection', testBrokerFeeCollection, 240 * ciTimeoutFactor);
  concurrentTest('BoostingForAsset', testBoostingSwap, 310 * ciTimeoutFactor);
  concurrentTest('FillOrKill', testFillOrKill, 300 * ciTimeoutFactor);
  concurrentTest('DCASwaps', testDCASwaps, 240 * ciTimeoutFactor);
  concurrentTest('CancelOrdersBatch', testCancelOrdersBatch, 300 * ciTimeoutFactor);
  concurrentTest('DepositChannelCreation', depositChannelCreation, 50 * ciTimeoutFactor);
  if (!process.env.SKIP_BLS_TESTS) {
    concurrentTest('BrokerLevelScreening', testBrokerLevelScreening, 360 * ciTimeoutFactor, inCi? 0 : 20);
  }
  concurrentTest('VaultSwaps', testVaultSwap, 340 * ciTimeoutFactor);
  concurrentTest('SpecialBitcoinSwaps', testSpecialBitcoinSwaps, 250 * ciTimeoutFactor);
  concurrentTest('DelegateFlip', testDelegate, 325 * ciTimeoutFactor);
  concurrentTest('SwapAndFundAccountViaCCM', testCcmSwapFundAccount, 240 * ciTimeoutFactor);
  concurrentTest('SignedRuntimeCall', testSignedRuntimeCall, 280 * ciTimeoutFactor);
  concurrentTest('Lending', lendingTest, 360 * ciTimeoutFactor);
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
