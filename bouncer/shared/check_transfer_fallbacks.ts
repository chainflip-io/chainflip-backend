import prisma from 'shared/utils/prisma_client';
import { getChainflipApi } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';

// TransferNativeFailed and TransferTokenFailed are events emitted by the EVM vault contracts
// when a transfer fails on-chain. The engine witnesses these and the State Chain responds by
// creating a "transfer fallback" — a new threshold-signed retry transaction — emitting
// TransferFallbackRequested and storing the call in FailedForeignChainCalls storage.
// Neither should ever happen during normal bouncer tests.

const TRANSFER_FALLBACK_EVENTS = [
  'EthereumIngressEgress.TransferFallbackRequested',
  'ArbitrumIngressEgress.TransferFallbackRequested',
  'TronIngressEgress.TransferFallbackRequested',
];

const FALLBACK_STORAGE_PALLETS = [
  'ethereumIngressEgress',
  'arbitrumIngressEgress',
  'tronIngressEgress',
] as const;

export async function checkNoTransferFallbacks(testContext: TestContext) {
  testContext.info('Checking that no EVM vault transfer fallbacks occurred during the tests');

  const fallbackEvents = await prisma.event.findMany({
    where: { name: { in: TRANSFER_FALLBACK_EVENTS } },
    include: { block: true },
    orderBy: { block: { height: 'asc' } },
  });

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
  for (const pallet of FALLBACK_STORAGE_PALLETS) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const entries = await (chainflipApi.query as any)[pallet].failedForeignChainCalls.entries();
    const nonEmpty = (entries as [unknown, { toJSON(): unknown[] }][]).filter(
      ([, calls]) => calls.toJSON().length > 0,
    );
    if (nonEmpty.length > 0) {
      throw new Error(
        `Pallet ${pallet} has ${nonEmpty.length} non-empty FailedForeignChainCalls ` +
          `entries at end of test run. This indicates pending unresolved transfer fallback(s).`,
      );
    }
  }
}
