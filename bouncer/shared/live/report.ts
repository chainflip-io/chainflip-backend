import { mkdirSync, writeFileSync } from 'fs';
import { dirname } from 'path';
import { Asset, bigintReplacer, sleep } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { queryEventsInRange, queryHighestIndexedBlock } from 'shared/utils/indexer_db';
import { BouncerNetwork } from 'shared/live/live_config';

// Structured record of everything that happened during a live swap (PRO-2959). The report is
// the primary output of `commands/live/submit_live_swap.ts`: there is no concrete test case
// yet, so we capture as much as possible for later automated or manual verification.

export type SwapEventRecord = {
  name: string;
  blockHeight: number;
  indexInBlock: number;
  args: unknown;
};

export type ExternalBalanceRecord = {
  asset: Asset;
  address: string;
  before: string;
  after?: string;
};

export type LiveSwapReport = {
  network: BouncerNetwork;
  genesisHash: string;
  startedAt: string;
  finishedAt?: string;
  sourceAsset: Asset;
  destAsset: Asset;
  /** Human units of the source asset. */
  amount: string;
  outcome: 'success' | 'refunded' | 'incomplete';
  brokerAccount: string;
  channel?: {
    channelId: string;
    depositAddress: string;
    issuedAtBlock: number;
    sourceChainExpiryBlock: string;
  };
  depositTxHash?: string;
  swap?: {
    swapRequestId: string;
    dcaChunks: number;
    /** One entry per executed chunk, straight from Swapping.SwapExecuted. */
    executed: {
      swapId: string;
      inputAmount: string;
      intermediateAmount?: string;
      outputAmount: string;
      networkFee: string;
      brokerFee: string;
      oracleDelta?: number;
    }[];
    egress?: {
      egressId: string;
      amount: string;
      fee: string;
      broadcastId?: number;
      broadcastSuccess: boolean;
    };
    refundEgress?: {
      egressId: string;
      amount: string;
      broadcastId?: number;
    };
  };
  externalBalances: {
    source: ExternalBalanceRecord;
    dest: ExternalBalanceRecord;
  };
  /** What our own LP did to fill this swap (absent with --skipLpFill). */
  lpFill?: JitFillReport & { lpAccount: string };
  phaseDurationsMs: Record<string, number>;
  /** Every indexed state-chain event that references this swap, in block order. */
  events: SwapEventRecord[];
};

/** Record of the just-in-time orders our LP placed for one swap, and what they bought. */
export type JitFillReport = {
  orders: {
    id: string;
    baseAsset: Asset;
    side: 'Buy' | 'Sell';
    tick: number;
    sellAmount: string;
    dispatchAt?: number;
    closeOrderAt: number;
  }[];
  fills: {
    orderId: string;
    baseAsset: Asset;
    side: 'Buy' | 'Sell';
    boughtAmount: string;
    collectedFees: string;
    /** Committed amount that came back unsold when the order auto-closed. */
    unsoldReturned: string;
  }[];
  /**
   * Legs of our own swap that our orders did not fill. Non-empty means the JIT fill lost the swap
   * to other liquidity, which the command treats as a failure even when the swap itself succeeded.
   */
  unfilledLegs?: { baseAsset: Asset; side: 'Buy' | 'Sell' }[];
  /** What the run put into the LP account from our wallet (fine units per asset). */
  deposited?: Record<string, string>;
  /** What the run took back out to our wallet: fills + unspent deposits. */
  withdrawals?: { asset: Asset; amount: string; egressId: string }[];
};

/**
 * Identifiers that tie state-chain events to one particular swap. Used to sweep the indexer
 * for the full event trail after the swap has completed.
 */
export type SwapIdentifiers = {
  channelId?: bigint;
  depositAddress?: string;
  swapRequestId?: bigint;
};

function normalise(value: unknown): string | undefined {
  if (typeof value === 'number' || typeof value === 'bigint') {
    return value.toString();
  }
  if (typeof value === 'string') {
    // The processor stores numeric fields either as JSON numbers or as hex strings.
    if (value.startsWith('0x')) {
      try {
        return BigInt(value).toString();
      } catch {
        return value.toLowerCase();
      }
    }
    return value.toLowerCase();
  }
  return undefined;
}

/** Recursively checks whether any (nested) field of `args` matches one of the identifiers. */
function referencesSwap(args: unknown, ids: Set<string>): boolean {
  if (args === null || typeof args !== 'object') {
    return normalise(args) !== undefined && ids.has(normalise(args)!);
  }
  return Object.values(args).some((value) => referencesSwap(value, ids));
}

/** Broadcasts carrying this swap's egresses, for pulling the broadcast events into the report. */
export type SwapBroadcast = { chain: string; broadcastId: number };

/**
 * Collects every indexed event in [fromBlock, toBlock] whose arguments reference the swap.
 * This deliberately matches on values rather than a hardcoded event-name list, so events we
 * didn't anticipate (boosts, refunds, ignored egresses, ...) end up in the report too.
 * Broadcast events don't reference any swap identifier, so they are matched separately by
 * broadcast id (a bare small integer would over-match in the generic value sweep).
 */
export async function collectSwapEvents(
  logger: Logger,
  fromBlock: number,
  toBlock: number,
  identifiers: SwapIdentifiers,
  broadcasts: SwapBroadcast[] = [],
): Promise<SwapEventRecord[]> {
  const ids = new Set<string>();
  // Insert identifiers through the same normalisation used to compare event args, so a hex
  // deposit address (which normalise() reduces to its decimal form) still matches.
  for (const value of [
    identifiers.channelId,
    identifiers.swapRequestId,
    identifiers.depositAddress,
  ]) {
    const normalised = value === undefined ? undefined : normalise(value);
    if (normalised !== undefined) {
      ids.add(normalised);
    }
  }

  // Don't race the ingest: it follows the chain with a small lag.
  for (let i = 0; (await queryHighestIndexedBlock()) < toBlock; i++) {
    if (i >= 120) {
      logger.warn(`Indexer still behind block ${toBlock}, the event sweep may be incomplete`);
      break;
    }
    await sleep(1000);
  }

  const events = await queryEventsInRange(fromBlock, toBlock);

  const matchesBroadcast = (event: { name: string; args: unknown }) => {
    const args = event.args as { broadcastId?: unknown } | null;
    const broadcastId = normalise(args?.broadcastId);
    return (
      broadcastId !== undefined &&
      broadcasts.some(
        (b) => event.name.startsWith(b.chain) && broadcastId === b.broadcastId.toString(),
      )
    );
  };

  const matching = events
    .filter((event) => referencesSwap(event.args, ids) || matchesBroadcast(event))
    .map((event) => ({
      name: event.name,
      blockHeight: event.blockHeight,
      indexInBlock: event.indexInBlock,
      args: event.args,
    }));
  logger.debug(
    `Collected ${matching.length}/${events.length} events referencing the swap in blocks ${fromBlock}..${toBlock}`,
  );
  return matching;
}

export function writeReport(logger: Logger, path: string, report: LiveSwapReport) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(report, bigintReplacer, 2));
  logger.info(`Swap report written to ${path}`);
}
