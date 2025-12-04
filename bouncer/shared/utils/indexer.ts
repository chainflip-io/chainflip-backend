import prisma from 'client';
import { sleep } from 'shared/utils';
import { z } from 'zod';
import { getChainflipApi } from './substrate';

export const hexString = z
  .string()
  .refine((v): v is `0x${string}` => /^0x[0-9a-fA-F]*$/.test(v), { message: 'Invalid hex string' });

export async function eventsFromCurrentBlock(): Promise<EventsFromBlock> {
  await using chainflip = await getChainflipApi();
  const header = await chainflip.rpc.chain.getHeader();
  return new EventsFromBlock(header.number.toNumber());
}

export class EventsFromBlock {
  startFrom: number;
  constructor(startFrom: number) {
    this.startFrom = startFrom;
  }
  async find<Z extends z.ZodTypeAny = z.ZodTypeAny>(
    name: `${string}.${string}` | `.${string}`,
    // {
      // test = () => true,
      // schema = z.any() as unknown as Z,
    // }
    // : {
      // test?: (data: z.output<Z>) => boolean;
      schema: Z
    // } = {},
  ): Promise<ValidatedEvent<Z>> {
    const event = await findEvent(name, this.startFrom, {schema});
    return event;
  }
}

type ValidatedEvent<Z extends z.ZodTypeAny> = Omit<Event, 'args'> & { args: z.output<Z> };

export const findEvent = async <Z extends z.ZodTypeAny = z.ZodTypeAny>(
  name: `${string}.${string}` | `.${string}`,
  startFromBlock: number,
  {
    test = () => true,
    schema = z.any() as unknown as Z,
  }: {
    test?: (data: z.output<Z>) => boolean;
    schema: Z;
  },
): Promise<ValidatedEvent<Z>> => {
  let event;

  while (!event) {
    const events = await prisma.event.findMany({
      where: {
        name: name.startsWith('.') ? { endsWith: name } : { equals: name },
        block: {
          height: {
            gte: startFromBlock
          }
        }
      },
    });

    event = events.find((e) => {
      const result = schema.safeParse(e.args);
      return result.success && test(result.data);
    });

    await sleep(250);
  }

  // return parsed args and replace
  event.args = schema.parse(event.args);

  return event.args as unknown as ValidatedEvent<Z>;
};