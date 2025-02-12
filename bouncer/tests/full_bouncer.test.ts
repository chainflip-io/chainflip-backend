import { describe } from 'vitest';
import { testBtcUtxoConsolidation } from './btc_utxo_consolidation';
import { testDeltaBasedIngress } from './delta_based_ingress';
import { testDoubleDeposit } from './double_deposit';
import { testRotatesThroughBtcSwap } from './rotates_through_btc_swap';
import { testRotateAndSwap } from './rotation_barrier';
import { testSolanaVaultSettingsGovernance } from './solana_vault_settings_governance';
import { serialTest } from '../shared/utils/vitest';
import { testGasLimitCcmSwaps } from './gaslimit_ccm';
import { testMinimumDeposit } from './minimum_deposit';
import { testSwapAfterDisconnection } from './swap_after_temp_disconnecting_chains';

// Tests that are ran by the ci-main-merge before the concurrent tests
describe('SerialTests1', () => {
  serialTest('GasLimitCcmSwaps', testGasLimitCcmSwaps, 1800);
});

// Tests that are run by the ci-main-merge after the concurrent tests
describe('SerialTests2', () => {
  serialTest('RotatesThroughBtcSwap', testRotatesThroughBtcSwap, 360);
  serialTest('BtcUtxoConsolidation', testBtcUtxoConsolidation, 200);
  serialTest('RotateAndSwap', testRotateAndSwap, 280);
  serialTest('DeltaBasedIngress', testDeltaBasedIngress, 800);
  serialTest('MinimumDeposit', testMinimumDeposit, 150);
  serialTest('SolanaVaultSettingsGovernance', testSolanaVaultSettingsGovernance, 120);
  serialTest('DoubleDeposit', testDoubleDeposit, 120);

  if (process.env.LOCALNET) {
    serialTest('SwapAfterDisconnection', testSwapAfterDisconnection, 1300);
  }
});
