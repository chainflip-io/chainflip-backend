import { describe } from 'vitest';
import { testBtcUtxoConsolidation } from './btc_utxo_consolidation';
import { testDeltaBasedIngress } from './delta_based_ingress';
import { testDoubleDeposit } from './double_deposit';
import { testRotatesThroughBtcSwap } from './rotates_through_btc_swap';
import { testRotateAndSwap } from './rotation_barrier';
import { testSolanaVaultSettingsGovernance } from './solana_vault_settings_governance';
import { serialTest } from '../shared/utils/vitest';

// Tests that are not run by the ci ("full bouncer") but can still be ran manually by using the command:
// pnpm vitest run -t <test_name>
describe('OtherTests', () => {
  serialTest('BtcUtxoConsolidation', testBtcUtxoConsolidation, 200);
  serialTest('DeltaBasedIngress', testDeltaBasedIngress, 800);
  serialTest('DoubleDeposit', testDoubleDeposit, 120);
  serialTest('RotatesThroughBtcSwap', testRotatesThroughBtcSwap, 360);
  serialTest('RotateAndSwap', testRotateAndSwap, 280);
  serialTest('testSolanaVaultSettingsGovernance', testSolanaVaultSettingsGovernance, 120);
});
