import { describe } from 'vitest';
import { testBtcUtxoConsolidation } from 'tests/btc_utxo_consolidation';
import { testRotatesThroughBtcSwap } from 'tests/rotates_through_btc_swap';
import { testRotateAndSwap } from 'tests/rotation_barrier';
import { testSolanaVaultSettingsGovernance } from 'tests/solana_vault_settings_governance';
import { serialTest } from 'shared/utils/vitest';
import { testGasLimitCcmSwaps } from 'tests/gaslimit_ccm';
import { testMinimumDeposit } from 'tests/minimum_deposit';
import { testSwapAfterDisconnection } from 'tests/swap_after_temp_disconnecting_chains';

// Tests that are ran by the ci-main-merge before the concurrent tests
describe('SerialTests1', () => {
  serialTest('GasLimitCcmSwaps', testGasLimitCcmSwaps, 1800);
});

// Tests that are run by the ci-main-merge after the concurrent tests
describe('SerialTests2', () => {
  serialTest('RotatesThroughBtcSwap', testRotatesThroughBtcSwap, 360);
  serialTest('BtcUtxoConsolidation', testBtcUtxoConsolidation, 200);
  serialTest('RotateAndSwap', testRotateAndSwap, 280);
  serialTest('MinimumDeposit', testMinimumDeposit, 150);
  serialTest('SolanaVaultSettingsGovernance', testSolanaVaultSettingsGovernance, 120);

  if (process.env.LOCALNET) {
    serialTest('SwapAfterDisconnection', testSwapAfterDisconnection, 1300);
  }
});
