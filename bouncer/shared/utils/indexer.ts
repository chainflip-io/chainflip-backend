import prisma from 'client';
import { sleep } from 'shared/utils';
import { z } from 'zod';

export const hexString = z
  .string()
  .refine((v): v is `0x${string}` => /^0x[0-9a-fA-F]*$/.test(v), { message: 'Invalid hex string' });

type ValidatedEvent<Z extends z.ZodTypeAny> = Omit<Event, 'args'> & { args: z.output<Z> };

export const findEvent = async <Z extends z.ZodTypeAny = z.ZodTypeAny>(
  name: `${string}.${string}` | `.${string}`,
  {
    test = () => true,
    schema = z.any() as unknown as Z,
  }: {
    test?: (data: z.output<Z>) => boolean;
    schema?: Z;
  } = {},
): Promise<ValidatedEvent<Z>> => {
  let event;

  while (!event) {
    const events = await prisma.event.findMany({
      where: { name: name.startsWith('.') ? { endsWith: name } : { equals: name } },
    });

    event = events.find((e) => test(schema.parse(e.args)));

    await sleep(250);
  }

  return event as unknown as ValidatedEvent<Z>;
};
