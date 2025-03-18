#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { runWithTimeoutAndExit } from '../shared/utils';

async function main() {
  const percent = Number(process.argv[2] ?? 40);
  await submitGovernanceExtrinsic((api) =>
    api.tx.bitcoinIngressEgress.updatePalletConfig([
      { SetNetworkFeeDeductionFromBoost: { percent } },
    ]),
  );
}
await runWithTimeoutAndExit(main(), 60);
