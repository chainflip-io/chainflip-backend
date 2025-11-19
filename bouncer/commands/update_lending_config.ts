#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { runWithTimeoutAndExit } from 'shared/utils';

async function main() {
  await submitGovernanceExtrinsic((api) =>
    api.tx.lendingPools.updatePalletConfig([
      {
        SetLtvThresholds: {
          ltvThresholds: {
            target: 800000,
            topup: 850000,
            soft_liquidation: 900000,
            soft_liquidation_abort: 880000,
            hard_liquidation: 950000,
            hard_liquidation_abort: 930000,
            low_ltv: 500000,
          },
        },
      },
      {
        SetFeeSwapThresholdUsd: '1',
      },
    ]),
  );
}
await runWithTimeoutAndExit(main(), 60);
