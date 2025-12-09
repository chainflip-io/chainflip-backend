import { z } from 'zod';
import { sleep } from 'shared/utils';
import prisma from './prisma_client';

export type EventName = `${string}.${string}` | `.${string}`;
type EventTime = {
  startFromBlock: number;
  endBeforeBlock?: number;
};

// ------------ Types for choosing an event of multiple alternatives   ---------------

export type EventDescription = { name: EventName; schema: z.ZodTypeAny };

export type EventDescriptions = Record<string, EventDescription>;

export type ChooseSingleEvent<S extends Record<string, EventDescription>> = {
  [K in keyof S]: {
    key: K;
    data: z.infer<S[K]['schema']>;
    blockHeight: number;
  };
}[keyof S];

// ------------ Querying the indexer database --------------
export const findOneEventOfMany = async <Descriptions extends EventDescriptions>(
  descriptions: Descriptions,
  timing: EventTime,
): Promise<ChooseSingleEvent<Descriptions>> => {
  let foundEventsKeyAndData: { key: string; data: unknown; blockHeight: number }[] = [];
  while (foundEventsKeyAndData.length === 0) {
    const matchingEvents = await prisma.event.findMany({
      where: {
        OR: Object.values(descriptions).map((d) => ({
          name: d.name.startsWith('.') ? { endsWith: d.name } : { equals: d.name },
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

    await sleep(2000);
  }

  if (foundEventsKeyAndData.length > 1) {
    throw new Error(
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
