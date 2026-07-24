import prisma from 'shared/utils/prisma_client';

// Data access for the event indexer, with two interchangeable backends:
// - Prisma straight to the local squid_archive Postgres (localnet default), or
// - the Postgraphile "indexer-gateway" that Chainflip runs over the same database for live
//   networks (e.g. https://indexer-perseverance.chainflip.io/graphql), selected via
//   INDEXER_GATEWAY_URL (defaulted automatically for BOUNCER_NETWORK=perseverance).
//
// Both serve the identical substrate-ingest schema, so the two IndexerBackend implementations
// below return the same shapes and ordering, and everything above this layer (findOneEventOfMany,
// the report sweep, all of ChainflipIO) is backend-agnostic.

const PERSEVERANCE_GATEWAY_URL = 'https://indexer-perseverance.chainflip.io/graphql';

function gatewayUrl(): string | undefined {
  return (
    process.env.INDEXER_GATEWAY_URL ??
    (process.env.BOUNCER_NETWORK === 'perseverance' ? PERSEVERANCE_GATEWAY_URL : undefined)
  );
}

export type IndexedEvent = {
  id: string;
  name: string;
  args: unknown;
  indexInBlock: number;
  blockHeight: number;
};

export type IndexedExtrinsic = {
  callId: string;
  blockHeight: number;
};

/** One disjunct of an event search: a name (or `.Suffix` name suffix) and optional call id. */
export type EventNameFilter = {
  name: string;
  callId?: string;
};

/**
 * A source of indexed chain data. Both backends order events by substrate-ingest id ascending,
 * which (see below) is block/event order, so results are identical regardless of backend.
 */
interface IndexerBackend {
  /** The highest block with indexed events (0 if none). */
  highestIndexedBlock(): Promise<number>;
  /** An extrinsic by transaction hash, or undefined if not (yet) indexed. */
  extrinsicByHash(txHash: string): Promise<IndexedExtrinsic | undefined>;
  /** Whether any event of the given block has been indexed yet. */
  anyEventAtBlock(blockHeight: number): Promise<boolean>;
  /** Events matching any name filter within [fromBlock, endBeforeBlock), in block order. */
  eventsByName(
    filters: EventNameFilter[],
    fromBlock: number,
    endBeforeBlock?: number,
  ): Promise<IndexedEvent[]>;
  /** Every event in [fromBlock, toBlock] (inclusive), in block order. */
  eventsInRange(fromBlock: number, toBlock: number): Promise<IndexedEvent[]>;
}

// ---------------- Postgraphile backend ----------------

// substrate-ingest ids start with the block height zero-padded to 10 digits (e.g. block
// "0012052137-354aa", event "0012052137-000152-354aa"), so id order is block/event order and
// height ranges are lexicographic id ranges.
const paddedHeight = (height: number) => height.toString().padStart(10, '0');
const heightOfId = (id: string) => Number(id.slice(0, 10));

async function gql<T>(query: string, variables: Record<string, unknown>): Promise<T> {
  const response = await fetch(gatewayUrl()!, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, variables }),
  });
  if (!response.ok) {
    throw new Error(`Indexer gateway returned ${response.status}: ${await response.text()}`);
  }
  const body = (await response.json()) as { data?: T; errors?: { message: string }[] };
  if (body.errors?.length) {
    throw new Error(`Indexer gateway query failed: ${body.errors[0].message}`);
  }
  return body.data!;
}

type EventNode = { id: string; name: string; args: unknown; indexInBlock: number };
type EventPage = {
  allEvents: { nodes: EventNode[]; pageInfo: { hasNextPage: boolean; endCursor: string | null } };
};

const toIndexedEvent = (node: EventNode): IndexedEvent => ({
  ...node,
  blockHeight: heightOfId(node.id),
});

/** Runs an allEvents query with the given filter, paginating until exhausted. */
async function gqlEvents(filter: Record<string, unknown>): Promise<IndexedEvent[]> {
  const events: IndexedEvent[] = [];
  let cursor: string | null = null;
  for (;;) {
    const page: EventPage = await gql<EventPage>(
      `query ($filter: EventFilter!, $cursor: Cursor) {
        allEvents(filter: $filter, orderBy: ID_ASC, first: 1000, after: $cursor) {
          nodes { id name args indexInBlock }
          pageInfo { hasNextPage endCursor }
        }
      }`,
      { filter, cursor },
    );
    events.push(...page.allEvents.nodes.map(toIndexedEvent));
    if (!page.allEvents.pageInfo.hasNextPage) {
      return events;
    }
    cursor = page.allEvents.pageInfo.endCursor;
  }
}

function gqlEventFilter(filters: EventNameFilter[], fromBlock: number, endBeforeBlock?: number) {
  const range: Record<string, unknown>[] = [
    { id: { greaterThanOrEqualTo: paddedHeight(fromBlock) } },
  ];
  if (endBeforeBlock !== undefined) {
    range.push({ id: { lessThan: paddedHeight(endBeforeBlock) } });
  }
  const names = filters.map((f) => ({
    and: [
      f.name.startsWith('.') ? { name: { endsWith: f.name } } : { name: { equalTo: f.name } },
      ...(f.callId !== undefined ? [{ callId: { equalTo: f.callId } }] : []),
    ],
  }));
  return { and: [...range, ...(names.length > 0 ? [{ or: names }] : [])] };
}

const graphqlBackend: IndexerBackend = {
  async highestIndexedBlock() {
    const data = await gql<{ allEvents: { nodes: { id: string }[] } }>(
      `query { allEvents(orderBy: ID_DESC, first: 1) { nodes { id } } }`,
      {},
    );
    const id = data.allEvents.nodes[0]?.id;
    return id ? heightOfId(id) : 0;
  },
  async extrinsicByHash(txHash) {
    const data = await gql<{ allExtrinsics: { nodes: { id: string; callId: string }[] } }>(
      `query ($hash: String!) {
        allExtrinsics(filter: { hash: { equalTo: $hash } }, first: 1) { nodes { id callId } }
      }`,
      { hash: txHash },
    );
    const node = data.allExtrinsics.nodes[0];
    return node ? { callId: node.callId, blockHeight: heightOfId(node.id) } : undefined;
  },
  async anyEventAtBlock(blockHeight) {
    const data = await gql<{ allEvents: { nodes: { id: string }[] } }>(
      `query ($filter: EventFilter!) { allEvents(filter: $filter, first: 1) { nodes { id } } }`,
      { filter: gqlEventFilter([], blockHeight, blockHeight + 1) },
    );
    return data.allEvents.nodes.length > 0;
  },
  async eventsByName(filters, fromBlock, endBeforeBlock) {
    return gqlEvents(gqlEventFilter(filters, fromBlock, endBeforeBlock));
  },
  async eventsInRange(fromBlock, toBlock) {
    return gqlEvents(gqlEventFilter([], fromBlock, toBlock + 1));
  },
};

// ---------------- Prisma backend ----------------

type PrismaEvent = {
  id: string;
  name: string;
  args: unknown;
  indexInBlock: number;
  block: { height: number };
};

const fromPrismaEvent = (event: PrismaEvent): IndexedEvent => ({
  id: event.id,
  name: event.name,
  args: event.args,
  indexInBlock: event.indexInBlock,
  blockHeight: event.block.height,
});

const prismaBackend: IndexerBackend = {
  async highestIndexedBlock() {
    const result = await prisma.event.findFirst({
      orderBy: { block: { height: 'desc' } },
      include: { block: true },
    });
    return result?.block.height ?? 0;
  },
  async extrinsicByHash(txHash) {
    const result = await prisma.extrinsic.findFirst({
      where: { hash: { equals: txHash } },
      include: { block: true },
    });
    return result ? { callId: result.callId, blockHeight: result.block.height } : undefined;
  },
  async anyEventAtBlock(blockHeight) {
    return (
      (await prisma.event.findFirst({ where: { block: { height: { equals: blockHeight } } } })) !==
      null
    );
  },
  async eventsByName(filters, fromBlock, endBeforeBlock) {
    const events = await prisma.event.findMany({
      where: {
        OR: filters.map((f) => ({
          name: f.name.startsWith('.') ? { endsWith: f.name } : { equals: f.name },
          callId: f.callId,
        })),
        block: { height: { gte: fromBlock, lt: endBeforeBlock } },
      },
      include: { block: true },
      // Ids encode (block height, event index), so id order matches the gateway's ID_ASC.
      orderBy: { id: 'asc' },
    });
    return events.map(fromPrismaEvent);
  },
  async eventsInRange(fromBlock, toBlock) {
    const events = await prisma.event.findMany({
      where: { block: { height: { gte: fromBlock, lte: toBlock } } },
      include: { block: true },
      orderBy: { id: 'asc' },
    });
    return events.map(fromPrismaEvent);
  },
};

// ---------------- Public interface ----------------

/** The active backend for the selected network (gateway for live, Prisma for localnet). */
function backend(): IndexerBackend {
  return gatewayUrl() ? graphqlBackend : prismaBackend;
}

/**
 * The highest block with indexed events. Uses the events table rather than the blocks table
 * to avoid races between block and event ingestion.
 */
export function queryHighestIndexedBlock(): Promise<number> {
  return backend().highestIndexedBlock();
}

/** Finds an extrinsic by transaction hash, or undefined if not (yet) indexed. */
export function queryExtrinsicByHash(txHash: string): Promise<IndexedExtrinsic | undefined> {
  return backend().extrinsicByHash(txHash);
}

/** Whether any event of the given block has been indexed yet. */
export function queryAnyEventAtBlock(blockHeight: number): Promise<boolean> {
  return backend().anyEventAtBlock(blockHeight);
}

/**
 * All events matching any of the name filters within [fromBlock, endBeforeBlock), in block
 * order. A filter's `callId` restricts it to events of one specific call.
 */
export function queryEventsByName(
  filters: EventNameFilter[],
  fromBlock: number,
  endBeforeBlock?: number,
): Promise<IndexedEvent[]> {
  return backend().eventsByName(filters, fromBlock, endBeforeBlock);
}

/** Every event in [fromBlock, toBlock] (inclusive), in block order. */
export function queryEventsInRange(fromBlock: number, toBlock: number): Promise<IndexedEvent[]> {
  return backend().eventsInRange(fromBlock, toBlock);
}
