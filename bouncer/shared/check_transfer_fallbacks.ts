import { getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Chains, ingressEgressPalletForChain } from 'shared/utils';
import { findAllEventsByName } from 'shared/utils/indexer';

// TransferNativeFailed and TransferTokenFailed are events emitted by the EVM vault contracts
// when a transfer fails on-chain. The engine witnesses these and the State Chain responds by
// creating a "transfer fallback" — a new threshold-signed retry transaction — emitting
// TransferFallbackRequested and storing the call in FailedForeignChainCalls storage.
// Neither should ever happen during normal bouncer tests.

// Chains that have EVM vault contracts with the TransferFallbackRequested mechanism.
const EVM_VAULT_CHAINS = [Chains.Ethereum, Chains.Arbitrum, Chains.Tron, Chains.Bsc] as const;

const TRANSFER_FALLBACK_EVENTS = EVM_VAULT_CHAINS.map(
  (chain) => `${chain}IngressEgress.TransferFallbackRequested`,
);

const INGRESS_EGRESS_STORAGE_PALLETS = EVM_VAULT_CHAINS.map(ingressEgressPalletForChain);

// FailedForeignChainCalls is shared between transfer fallbacks and failed CCM broadcasts.
// BroadcastActions is set to { ccmBroadcast: null } for CCM broadcasts and absent for
// transfer fallbacks, so we use it to distinguish the two.
async function isCcmBroadcast(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  palletQuery: any,
  broadcastId: number,
): Promise<boolean> {
  const action = (await palletQuery.broadcastActions(broadcastId)).toJSON() as Record<
    string,
    unknown
  > | null;
  return action?.ccmBroadcast !== undefined;
}

export async function checkNoTransferFallbacks(testContext: TestContext) {
  testContext.info('Checking that no EVM vault transfer fallbacks occurred during the tests');

  const fallbackEvents = (
    await Promise.all(TRANSFER_FALLBACK_EVENTS.map(findAllEventsByName))
  ).flat();

  if (fallbackEvents.length > 0) {
    const occurrences = fallbackEvents
      .map((e) => `block ${e.block.height} [${e.name}]: ${JSON.stringify(e.args)}`)
      .join('\n  ');
    throw new Error(
      `EVM vault transfer fallback(s) were triggered during the tests. This means a vault egress ` +
        `transaction failed on-chain (TransferNativeFailed or TransferTokenFailed on the vault contract). ` +
        `Occurrences:\n  ${occurrences}`,
    );
  }

  const chainflipApi = await getChainflipApi();
  for (const pallet of INGRESS_EGRESS_STORAGE_PALLETS) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const palletQuery = (chainflipApi.query as any)[pallet];
    const entries = await palletQuery.failedForeignChainCalls.entries();
    const allCalls = (entries as [unknown, { toJSON(): { broadcastId: number }[] }][]).flatMap(
      ([, calls]) => calls.toJSON(),
    );

    const transferFallbackCalls = (
      await Promise.all(
        allCalls.map(async (call) =>
          (await isCcmBroadcast(palletQuery, call.broadcastId)) ? null : call,
        ),
      )
    ).filter((call) => call !== null);

    if (transferFallbackCalls.length > 0) {
      throw new Error(
        `Pallet ${pallet} has ${transferFallbackCalls.length} non-CCM FailedForeignChainCalls ` +
          `entries at end of test run. This indicates pending unresolved transfer fallback(s): ` +
          `${JSON.stringify(transferFallbackCalls)}`,
      );
    }
  }
}
