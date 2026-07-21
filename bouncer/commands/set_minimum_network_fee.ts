#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { runWithTimeoutAndExit } from 'shared/utils';

async function main() {
  const minFee = BigInt(process.argv[2] ?? 500_000);
  await submitGovernanceExtrinsic((api) =>
    api.tx.swapping.updatePalletConfig([
      { type: 'SetNetworkFee', value: { minimum: minFee } },
      { type: 'SetInternalSwapNetworkFee', value: { minimum: minFee } },
    ]),
  );
}
await runWithTimeoutAndExit(main(), 60);
