import { SubmittableExtrinsic } from '@polkadot/api/types';
import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { getChainflipApi, handleSubstrateError, snowWhiteMutex } from './utils';

const chainflip = await getChainflipApi();

const snowWhiteUri =
  process.env.SNOWWHITE_URI ??
  'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

await cryptoWaitReady();

const keyring = new Keyring({ type: 'sr25519' });

export const snowWhite = keyring.createFromUri(snowWhiteUri);

export async function submitGovernanceExtrinsic(
  extrinsic: SubmittableExtrinsic<'promise'>,
  preAuthorise = 0,
) {
  return snowWhiteMutex.runExclusive(async () =>
    chainflip.tx.governance
      .proposeGovernanceExtrinsic(extrinsic, preAuthorise)
      .signAndSend(snowWhite, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}
