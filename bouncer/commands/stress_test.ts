#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will trigger an EthereumBroadcaster signing stress test to be executed on the chainflip state-chain
// The argument specifies the number of requested signatures
// For example: ./commands/stress_test.ts 3
// will initiate a stress test generating 3 signatures

import { runWithTimeoutAndExit } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function main(): Promise<void> {
  const signaturesCount = process.argv[2];

  await submitGovernanceExtrinsic((api) => {
    const stressTest = api.tx.ethereumBroadcaster.stressTest(signaturesCount);
    return api.tx.governance.callAsSudo(stressTest);
  });

  console.log('Requesting ' + signaturesCount + ' ETH signatures');
}

await runWithTimeoutAndExit(main(), 10);
