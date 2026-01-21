import { z } from 'zod';
import { sleep } from 'shared/utils';
import prisma from 'shared/utils/prisma_client';
import { Logger } from 'shared/utils/logger';
import { JsonValue } from 'generated/prisma/runtime/library';

// ------------ primitives event types ------------

export type EventName = `${string}.${string}` | `.${string}`;
type EventTime = {
  startFromBlock: number;
  endBeforeBlock?: number;
};

export type EventFilter = {
  txHash?: string;
};

export type EventDescription = {
  name: EventName;
  schema?: z.ZodTypeAny;
  additionalFilter?: EventFilter;
};

export type DataOf<D extends EventDescription> = D['schema'] extends z.ZodTypeAny
  ? z.infer<D['schema']>
  : JsonValue;

export type EventDescriptions = Record<string, EventDescription>;

// ------------ Event queries -------------

type OneOfEventsQuery = { oneOf: EventDescriptions };
type AllOfEventsQuery = { allOf: EventDescriptions };

/** Which events we want to wait for, there are three options:
 * - waiting for a single event
 * - waiting for one of multiple events
 * - waiting for all of multiple events
 */
export type EventQuery = EventDescription | OneOfEventsQuery | AllOfEventsQuery;

// ------------ Result types of event queries  ---------------

export type SingleEventResult<Key, Event extends EventDescription> = {
  key: Key;
  data: DataOf<Event>;
  blockHeight: number;
};

export type OneOfEventsResult<Descriptions extends Record<string, EventDescription>> = {
  [Key in keyof Descriptions]: SingleEventResult<Key, Descriptions[Key]>;
}[keyof Descriptions];

export type AllOfEventsResult<Descriptions extends Record<string, EventDescription>> = {
  [Key in keyof Descriptions]: SingleEventResult<Key, Descriptions[Key]>;
};

export type ResultOfEventQuery<Q extends EventQuery> = Q extends OneOfEventsQuery
  ? OneOfEventsResult<Q['oneOf']>
  : Q extends AllOfEventsQuery
    ? AllOfEventsResult<Q['allOf']>
    : Q extends EventDescription
      ? SingleEventResult<'event', Q>
      : never;

// ------------ Querying for block height --------------

export const highestBlock = async (): Promise<number> => {
  const result = await prisma.block.findFirst({
    orderBy: {
      height: 'desc',
    },
  });
  return result?.height ?? 0;
};

// ------------ Querying for transaction hashes --------------

async function findTxHash(txhash: string) {
  // eslint-disable-next-line no-constant-condition
  while (true) {
    const result = await prisma.extrinsic.findFirst({
      where: {
        hash: { equals: txhash },
      },
      include: {
        block: true,
      },
    });

    if (result) {
      return result;
    }

    await sleep(500);
  }
}

/**
 * Searches for a block that contains the given txhash in the indexer database.
 *
 * WARNING: This expects the txhash to be eventually available, and will loop indefinitely if it isn't found.
 *
 * @param txhash transaction hash to look for
 * @returns block height of the block where the transaction was found
 */
export async function blockHeightOfTransactionHash(txhash: string): Promise<number> {
  return (await findTxHash(txhash)).block.height;
}

// ------------ Querying for events --------------
export const findOneEventOfMany = async <Descriptions extends EventDescriptions>(
  logger: Logger,
  descriptions: Descriptions,
  timing: EventTime,
): Promise<OneOfEventsResult<Descriptions>> => {
  // before searching for events, we collect all call ids for events that have an associated txhash
  const callIdsList: { [x: string]: string | undefined }[] = await Promise.all(
    Object.entries(descriptions).map(([key, description]) =>
      description.additionalFilter?.txHash
        ? findTxHash(description.additionalFilter.txHash).then((tx) => ({ [key]: tx.callId }))
        : Promise.resolve({ [key]: undefined }),
    ),
  );
  const callIds = Object.assign(callIdsList) as { [x: string]: string | undefined };

  // now we search for all events, and if provided we require
  //  - the block height to be restricted to the ones allowed by `timings`
  //  - the callId that's associated with the event to be the one belonging to the provided `txHash`
  let foundEventsKeyAndData: { key: string; data: JsonValue; blockHeight: number }[] = [];
  while (foundEventsKeyAndData.length === 0) {
    const matchingEvents = await prisma.event.findMany({
      where: {
        OR: Object.entries(descriptions).map(([key, d]) => ({
          name: d.name.startsWith('.') ? { endsWith: d.name } : { equals: d.name },
          callId: callIds[key],
        })),
        block: {
          height: {
            gte: timing.startFromBlock,
            lt: timing.endBeforeBlock,
          },
        },
      },
      include: {
        block: true,
      },
    });

    if (matchingEvents) {
      foundEventsKeyAndData = matchingEvents.flatMap((event) => {
        const schemas = Object.entries(descriptions).flatMap(([key, d]) =>
          event.name.includes(d.name) ? [{ key, schema: d.schema }] : [],
        );
        if (schemas.length === 0) {
          throw new Error(
            `Unexpected internal error in 'findOneOfMany': there where no event descriptions found matching the chosen event ${JSON.stringify(event)}. The database query might be off.`,
          );
        }

        // Even though we found all events that match the given names, we have to check whether they
        // also match the given schema.
        const parsingResults = schemas.flatMap(({ key, schema }) => {
          if (!schema) {
            return [{ key, data: event.args, blockHeight: event.block.height }];
          }

          const r = schema.safeParse(event.args);
          return r.success ? [{ key, data: r.data, blockHeight: event.block.height }] : [];
        });

        if (parsingResults.length > 1) {
          throw new Error(
            `Single event successfully matched against multiple event descriptions.\n\nevent:${JSON.stringify(event)}\n\ndescription keys:${JSON.stringify(parsingResults.map((r) => r.key))}`,
          );
        }

        return parsingResults;
      });
    }

    // we wait two additional CF blocks to be indexed before we error out in case we couldn't find the event(s) we were looking for
    if (timing.endBeforeBlock && (await highestBlock()) > timing.endBeforeBlock + 2) {
      throw new Error(
        `Did not find any of the events in ${JSON.stringify(Object.values(descriptions).map((v) => v.name))} in block range ${timing.startFromBlock}..${timing.endBeforeBlock}`,
      );
    }

    await sleep(500);
  }

  if (foundEventsKeyAndData.length > 1) {
    logger.warn(
      `Found multiple events matching event descriptions, but only one was expected. Found: ${JSON.stringify(foundEventsKeyAndData)}`,
    );
  }

  return foundEventsKeyAndData[0];
};

// ------------ General fix  ---------------
// the following fixes the "TypeError: Do not know how to serialize a BigInt" error.
// Whenever the indexer is used to find events it should be included, thus it's here in this file.
declare global {
  interface BigInt {
    toJSON(): string;
  }
}

// eslint-disable-next-line no-extend-native, func-names
BigInt.prototype.toJSON = function () {
  return this.toString();
};
