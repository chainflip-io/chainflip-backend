#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { runWithTimeoutAndExit } from '../shared/utils';

async function main() {
  const minFee = Number(process.argv[2] ?? 500_000);
  await submitGovernanceExtrinsic((api) =>
    api.tx.swapping.updatePalletConfig([
      { SetMinimumNetworkFee: { minFee } },
      { SetInternalSwapMinimumNetworkFee: { minFee } },
    ]),
  );
}
await runWithTimeoutAndExit(main(), 60);
