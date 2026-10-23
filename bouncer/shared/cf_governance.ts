import type { SubmittableExtrinsic } from '@polkadot/api/types';
import type { IKeyringPair } from 'dedot/types';
import Keyring from 'polkadot/keyring';
import { cfMutex, waitForExt } from 'shared/utils';
import {
  DisposableApiPromise,
  getChainflipPolkadotApi,
  getChainflipApi,
} from 'shared/utils/substrate';
import { ChainflipClient, ChainflipExtrinsic, signSendAndWait } from 'shared/utils/dedot';
import type {
  PalletCfGovernanceExecutionMode,
  StateChainRuntimeRuntimeCallLike,
} from 'generated/chaintypes/chainflip-node';
import { Logger } from 'pino';
import { globalLogger } from 'shared/utils/logger';
import { findOneEventOfMany } from 'shared/utils/indexer';
import { governanceProposedEvent } from 'generated/events/governance/proposed';

const snowWhiteUri =
  process.env.SNOWWHITE_URI ??
  'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

const keyring = new Keyring({ type: 'sr25519' });

export const snowWhite = keyring.createFromUri(snowWhiteUri);

/// Secondary council member for tests that need a multi-member council.
/// The account is materialised on first use via `inc_sufficients` when added
/// by `new_membership_set`; governance members don't pay fees so it doesn't
/// need funding.
export const secondGovernanceMember = keyring.createFromUri('//cf-governance-2');

async function findProposalId(
  logger: Logger,
  txHash: string,
  startFromBlock: number,
): Promise<number> {
  // Use the indexer to find the Governance.Proposed event. This avoids SCALE decoding of the
  // submit result's events, which fails at runtime upgrade boundaries.
  const { name, schema } = governanceProposedEvent;
  const event = await findOneEventOfMany(
    logger,
    { event: { name, schema, txHash } },
    { startFromBlock },
  );
  logger.debug(`Governance extrinsic proposal ID: ${event.data}`);
  return event.data;
}

/**
 * @deprecated Legacy polkadot.js governance path. Prefer the dedot {@link submitGovernanceExtrinsic}
 * (or `ChainflipIO.submitGovernance`). Keeping around for now due to Dedot having a bug when encoding
 * some config-update shapes.
 */
export async function submitGovernanceExtrinsicPolkadot(
  call: (
    api: DisposableApiPromise,
  ) => SubmittableExtrinsic<'promise'> | Promise<SubmittableExtrinsic<'promise'>>,
  logger: Logger = globalLogger,
  preAuthorise = 0,
): Promise<number> {
  await using api = await getChainflipPolkadotApi();
  const extrinsic = await call(api);

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

  return findProposalId(logger, txHash, txBlockNumber);
}

/**
 * dedot variant of {@link submitGovernanceExtrinsicPolkadot}: the inner extrinsic is built from the
 * compile-time-typed dedot client (`client.tx.<pallet>.<call>(...)`) and proposed via dedot's
 * `governance.proposeGovernanceExtrinsic`. Returns the proposal id once the `Governance.Proposed`
 * event is found (via the indexer).
 *
 * NOTE: transitional — lives alongside the polkadot.js version while call sites are migrated.
 */
export async function submitGovernanceExtrinsic(
  call: (client: ChainflipClient) => ChainflipExtrinsic | Promise<ChainflipExtrinsic>,
  logger: Logger = globalLogger,
  preAuthorise = 0,
): Promise<number> {
  await using client = await getChainflipApi();
  const inner = await call(client);

  // The polkadot.js path passed `preAuthorise` as a number that SCALE-encoded the
  // ExecutionMode enum (0 -> Automatic, 1 -> Manual). Translate to the typed dedot variant.
  const execution: PalletCfGovernanceExecutionMode = preAuthorise === 0 ? 'Automatic' : 'Manual';

  logger.debug(`Submitting governance extrinsic`);

  // `inner.call` is the concrete RuntimeCall built inside the closure; the `ChainflipExtrinsic`
  // supertype widens its type to `IRuntimeTxCall`, so narrow it back.
  const proposal = client.tx.governance.proposeGovernanceExtrinsic(
    inner.call as StateChainRuntimeRuntimeCallLike,
    execution,
  );
  const result = await signSendAndWait(client, proposal, snowWhite, snowWhiteUri);

  const txHash = `${result.txHash}`;
  const txBlockNumber = result.status.value.blockNumber;
  logger.debug(`Governance extrinsic tx hash: ${txHash}, tx block: ${txBlockNumber}`);

  return findProposalId(logger, txHash, txBlockNumber);
}

/// Submit `governance.approve(proposalId)` signed by `approver`.
export async function submitGovernanceApproval(
  proposalId: number,
  approver: IKeyringPair,
  logger: Logger = globalLogger,
): Promise<void> {
  await using client = await getChainflipApi();

  logger.debug(
    `Submitting governance approval for proposal ${proposalId} from ${approver.address}`,
  );

  await signSendAndWait(
    client,
    client.tx.governance.approve(proposalId),
    approver,
    approver.address,
    undefined,
    true,
  );
  logger.debug(`Governance approval for proposal ${proposalId} finalised`);
}
