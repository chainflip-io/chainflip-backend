import type { SubmittableExtrinsic } from '@polkadot/api/types';
import Keyring from 'polkadot/keyring';
import { cfMutex, waitForExt } from 'shared/utils';
import { DisposableApiPromise, getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'pino';
import { globalLogger } from 'shared/utils/logger';
import { findOneEventOfMany } from 'shared/utils/indexer';
import { governanceProposed } from 'generated/events/governance/proposed';

const snowWhiteUri =
  process.env.SNOWWHITE_URI ??
  'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

const keyring = new Keyring({ type: 'sr25519' });

export const snowWhite = keyring.createFromUri(snowWhiteUri);

export async function submitExistingGovernanceExtrinsic(
  extrinsic: SubmittableExtrinsic<'promise'>,
  logger: Logger = globalLogger,
  preAuthorise = 0,
): Promise<number> {
  await using api = await getChainflipApi();

  logger.debug(`Submitting governance extrinsic`);

  const release = await cfMutex.acquire(snowWhiteUri);
  const { promise, waiter } = waitForExt(api, logger, 'Finalized', release);

  const nonce = (await api.rpc.system.accountNextIndex(snowWhite.address)) as unknown as number;
  const unsub = await api.tx.governance
    .proposeGovernanceExtrinsic(extrinsic, preAuthorise)
    .signAndSend(snowWhite, { nonce }, waiter);

  const submitResult = await promise;
  unsub();

  const txHash = `${submitResult.txHash}`;
  const txBlockNumber = (
    await api.rpc.chain.getHeader(submitResult.status.asFinalized)
  ).number.toNumber();
  logger.debug(`Governance extrinsic tx hash: ${txHash}, tx block: ${txBlockNumber}`);

  // Use the indexer to find the Governance.Proposed event. This avoids
  // SCALE decoding of submitResult.events which fails at runtime upgrade boundaries.
  const event = await findOneEventOfMany(
    logger,
    {
      event: {
        name: 'Governance.Proposed',
        schema: governanceProposed,
        txHash,
      },
    },
    { startFromBlock: txBlockNumber },
  );

  const proposalId = event.data;
  logger.debug(`Governance extrinsic proposal ID: ${proposalId}`);
  return proposalId;
}

export async function submitGovernanceExtrinsic(
  call: (
    api: DisposableApiPromise,
  ) => SubmittableExtrinsic<'promise'> | Promise<SubmittableExtrinsic<'promise'>>,
  logger: Logger = globalLogger,
  preAuthorise = 0,
): Promise<number> {
  await using api = await getChainflipApi();
  return submitExistingGovernanceExtrinsic(await call(api), logger, preAuthorise);
}
