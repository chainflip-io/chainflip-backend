#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will trigger an EthereumBroadcaster signing stress test to be executed on the chainflip state-chain
// The argument specifies the number of requested signatures
// For example: ./commands/stress_test.ts 3
// will initiate a stress test generating 3 signatures

import { getChainflipApi, runWithTimeout } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function main(): Promise<void> {
  const signaturesCount = process.argv[2];

  const api = await getChainflipApi();
  const stressTest = api.tx.ethereumBroadcaster.stressTest(signaturesCount);
  const sudoCall = api.tx.governance.callAsSudo(stressTest);

  await submitGovernanceExtrinsic(sudoCall);

  console.log('Requesting ' + signaturesCount + ' ETH signatures');

  process.exit(0);
}

runWithTimeout(main(), 10000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
