// INSTRUCTIONS
//
// This command takes no arguments.
// It will force a rotation on the chainflip state-chain
// For example: pnpm tsx ./commands/vault_rotation.ts

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  const snowwhite = keyring.createFromUri(snowwhiteUri);
  const chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });

  console.log('Forcing rotation');
  await chainflip.tx.governance
    .proposeGovernanceExtrinsic(chainflip.tx.validator.forceRotation())
    .signAndSend(snowwhite);

  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
