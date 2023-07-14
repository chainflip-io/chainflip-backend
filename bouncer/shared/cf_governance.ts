import { SubmittableExtrinsic } from '@polkadot/api/types';
import { getChainflipApi, handleSubstrateError, snowWhiteMutex } from './utils';
import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';

const chainflip = await getChainflipApi();

const snowWhiteUri =
  process.env.SNOWWHITE_URI ??
  'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

await cryptoWaitReady();

const keyring = new Keyring({ type: 'sr25519' });

const snowWhite = keyring.createFromUri(snowWhiteUri);

export async function submitGovernanceExtrinsic(extrinsic: SubmittableExtrinsic<'promise'>) {
  return await snowWhiteMutex.runExclusive(async () => {
    return chainflip.tx.governance
      .proposeGovernanceExtrinsic(extrinsic)
      .signAndSend(snowWhite, { nonce: -1 }, handleSubstrateError(chainflip));
  });
}
