import type { SubmittableExtrinsic } from '@polkadot/api/types';
import { ApiPromise, HttpProvider } from '@polkadot/api';
import Keyring from '../polkadot/keyring';
import { handleSubstrateError, snowWhiteMutex } from './utils';
import { CHAINFLIP_HTTP_ENDPOINT } from './utils/substrate';

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
  const httpApi = await ApiPromise.create({
    provider: new HttpProvider(CHAINFLIP_HTTP_ENDPOINT),
    noInitWarn: true,
  });

  const extrinsic = await cb(httpApi);
  await snowWhiteMutex.runExclusive(async () => {
    const nonce = await httpApi.rpc.system.accountNextIndex(snowWhite.address);
    await httpApi.tx.governance
      .proposeGovernanceExtrinsic(extrinsic, preAuthorise)
      .signAndSend(snowWhite, { nonce }, handleSubstrateError(httpApi));
  });
}
