import type { SubmittableExtrinsic } from '@polkadot/api/types';
import Keyring from 'polkadot/keyring';
import { cfMutex, waitForExt } from 'shared/utils';
import { DisposableApiPromise, getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'pino';
import { globalLogger } from 'shared/utils/logger';

const snowWhiteUri =
  process.env.SNOWWHITE_URI ??
  'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

const keyring = new Keyring({ type: 'sr25519' });

export const snowWhite = keyring.createFromUri(snowWhiteUri);

export async function submitGovernanceExtrinsic(
  call: (
    api: DisposableApiPromise,
  ) => SubmittableExtrinsic<'promise'> | Promise<SubmittableExtrinsic<'promise'>>,
  logger: Logger = globalLogger,
  preAuthorise = 0,
): Promise<number> {
  await using api = await getChainflipApi();

  logger.debug(`Submitting governance extrinsic`);

  const extrinsic = await call(api);
  const release = await cfMutex.acquire(snowWhiteUri);
  const { promise, waiter } = waitForExt(api, logger, 'InBlock', release);

  const nonce = (await api.rpc.system.accountNextIndex(snowWhite.address)) as unknown as number;
  const unsub = await api.tx.governance
    .proposeGovernanceExtrinsic(extrinsic, preAuthorise)
    .signAndSend(snowWhite, { nonce }, waiter);

  const events = await promise;
  unsub();

  const proposalId = events
    .find(({ event: { section, method } }) => section === 'governance' && method === 'Proposed')
    ?.event.data[0].toPrimitive()
    ?.valueOf();
  if (typeof proposalId !== 'number') {
    logger.error(logger, `Failed to find proposal ID in events: ${events}`);
    throw new Error('Failed to find proposal ID in events');
  }

  logger.debug(`Governance extrinsic proposal ID: ${proposalId}`);
  return proposalId;
}
