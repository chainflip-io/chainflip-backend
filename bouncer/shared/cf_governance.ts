import type { SubmittableExtrinsic } from '@polkadot/api/types';
import type { ApiPromise } from '@polkadot/api';
import Keyring from '../polkadot/keyring';
import { getChainflipApi, handleSubstrateError, snowWhiteMutex } from './utils';

const snowWhiteUri =
  process.env.SNOWWHITE_URI ??
  'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

const keyring = new Keyring({ type: 'sr25519' });

export const snowWhite = keyring.createFromUri(snowWhiteUri);

export async function submitGovernanceExtrinsic(
  cb: (
    api: ApiPromise,
  ) => SubmittableExtrinsic<'promise'> | Promise<SubmittableExtrinsic<'promise'>>,
  preAuthorise = 0,
) {
  await using chainflip = await getChainflipApi();
  const extrinsic = await cb(chainflip);
  await snowWhiteMutex.runExclusive(async () =>
    chainflip.tx.governance
      .proposeGovernanceExtrinsic(extrinsic, preAuthorise)
      .signAndSend(snowWhite, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}
