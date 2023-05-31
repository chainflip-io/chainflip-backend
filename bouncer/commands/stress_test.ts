// INSTRUCTIONS
//
// This command takes one argument.
// It will trigger an EthereumBroadcaster signing stress test to be executed on the chainflip state-chain
// The argument specifies the number of requested signatures
// For example: pnpm tsx ./commands/stress_test.ts 3
// will initiate a stress test generating 3 signatures

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  const signaturesCount = process.argv[2];
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  const snowwhite = keyring.createFromUri(snowwhiteUri);
  const api = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });
  const stressTest = api.tx.ethereumBroadcaster.stressTest(signaturesCount);
  const sudoCall = api.tx.governance.callAsSudo(stressTest);
  const proposal = api.tx.governance.proposeGovernanceExtrinsic(sudoCall);
  await proposal.signAndSend(snowwhite);
  process.exit(0);
}

runWithTimeout(main(), 10000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
