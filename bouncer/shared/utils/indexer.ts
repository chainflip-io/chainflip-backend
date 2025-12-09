import { z } from 'zod';
import { sleep } from 'shared/utils';
import prisma from './prisma_client';
import { Event } from './substrate';
import { ChooseSingleEvent, Err, EventDescriptions, Ok, Result } from './chainflip_io';

type ValidatedEvent<Z extends z.ZodTypeAny> = Omit<Event, 'args'> & {
  args: z.output<Z>;
  blockHeight: number;
};

type EventName = `${string}.${string}` | `.${string}`;
type EventTime = {
  startFromBlock: number;
  endBeforeBlock?: number;
};

export const findEvent = async <Z extends z.ZodTypeAny = z.ZodTypeAny>(
  name: EventName,
  timing: EventTime,
  schema: Z = z.any() as unknown as Z,
): Promise<ValidatedEvent<Z>> => {
  let event;

  while (!event) {
    const events = await prisma.event.findMany({
      where: {
        name: name.startsWith('.') ? { endsWith: name } : { equals: name },
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

    event = events.find((e) => {
      const result = schema.safeParse(e.args);
      return result.success;
    });

    await sleep(250);
  }

  const result = event as unknown as ValidatedEvent<Z>;

  // parse results from events and put them into the result
  result.args = schema.parse(event.args);
  result.blockHeight = event.block.height;

  return event as unknown as ValidatedEvent<Z>;
};

export const findOneEventOfMany = async <Descriptions extends EventDescriptions>(
  descriptions: Descriptions,
  timing: EventTime,
): Promise<ChooseSingleEvent<Descriptions>> => {
  let foundEventsKeyAndData: { key: string; data: any; blockHeight: number }[] = [];
  while (foundEventsKeyAndData.length == 0) {
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

    foundEventsKeyAndData = matchingEvents.flatMap((event) => {
      const schemas = Object.entries(descriptions).flatMap(([key, d]) =>
        event.name.includes(d.name) ? [{ key, schema: d.schema }] : [],
      );
      if (schemas.length == 0) {
        console.log(`Schemas are empty!!!!`);
      }
      const parsingResults = schemas.flatMap(({ key, schema }) => {
        console.log(`found matching event with ${JSON.stringify(event.args)}`);
        const r = schema.parse(event.args);
        return true ? [{ key, data: r, blockHeight: event.block.height }] : [];
      });

      if (parsingResults.length > 1) {
        throw new Error(
          `Single event successfully matched against multiple event descriptions.\n\nevent:${JSON.stringify(event)}\n\ndescription keys:${JSON.stringify(parsingResults.map((r) => r.key))}`,
        );
      }

      return parsingResults;
    });

    console.log(`found events: ${foundEventsKeyAndData}`);

    await sleep(2000);
  }

  if (foundEventsKeyAndData.length > 1) {
    throw new Error(
      `Found multiple events matching event descriptions, but only one was expected. Found: ${JSON.stringify(foundEventsKeyAndData)}`,
    );
  }

  return foundEventsKeyAndData[0];
};

export const findGoodOrBadEvent = async <
  Z1 extends z.ZodTypeAny = z.ZodTypeAny,
  Z2 extends z.ZodTypeAny = z.ZodTypeAny,
>(
  timing: EventTime,
  events: {
    good: EventName;
    goodSchema: Z1;
    bad: EventName;
    badSchema: Z2;
  },
): Promise<Result<ValidatedEvent<Z1>, ValidatedEvent<Z2>>> => {
  let goodEvent;
  let badEvent;

  while (!goodEvent && !badEvent) {
    const goodEvents = await prisma.event.findMany({
      where: {
        OR: [
          {
            name: events.good.startsWith('.') ? { endsWith: events.good } : { equals: events.good },
          },
          { name: events.bad.startsWith('.') ? { endsWith: events.bad } : { equals: events.bad } },
        ],
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

    goodEvent = goodEvents.find((e) => {
      const result = events.goodSchema.safeParse(e.args);
      return result.success && e.name.includes(events.good);
    });

    badEvent = goodEvents.find((e) => {
      const result = events.badSchema.safeParse(e.args);
      return result.success && e.name.includes(events.bad);
    });

    await sleep(250);
  }

  if (goodEvent && !badEvent) {
    const result = goodEvent as unknown as ValidatedEvent<Z1>;

    // parse results from events and put them into the result
    result.args = events.goodSchema.parse(goodEvent.args);
    result.blockHeight = goodEvent.block.height;

    return Ok(goodEvent as unknown as ValidatedEvent<Z1>);
  } else if (!goodEvent && badEvent) {
    const result = badEvent as unknown as ValidatedEvent<Z1>;

    // parse results from events and put them into the result
    result.args = events.badSchema.parse(badEvent.args);
    result.blockHeight = badEvent.block.height;

    return Err(badEvent as unknown as ValidatedEvent<Z1>);
  }

  throw new Error(
    `Encountered both good and bad event when waiting for one of them.\ngood:\n${JSON.stringify(goodEvent)}\n\nbad:\n${JSON.stringify(badEvent)}`,
  );
};
