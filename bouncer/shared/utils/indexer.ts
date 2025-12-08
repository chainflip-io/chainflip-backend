import prisma from './prisma_client';
import {
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  extractExtrinsicResult,
  lpMutex,
  sleep,
} from 'shared/utils';
import { z } from 'zod';
import { DisposableApiPromise, getChainflipApi } from './substrate';
import { Event } from './substrate';
import { globalLogger } from './logger';
import { KeyringPair } from '@polkadot/keyring/types';



type ValidatedEvent<Z extends z.ZodTypeAny> = Omit<Event, 'args'> & {
  args: z.output<Z>;
  blockHeight: number;
};

export const findEvent = async <Z extends z.ZodTypeAny = z.ZodTypeAny>(
  name: `${string}.${string}` | `.${string}`,
  timing: {
    startFromBlock: number;
    endBeforeBlock?: number;
  },
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
      return result.success && test(result.data);
    });

    await sleep(250);
  }

  const result = event as unknown as ValidatedEvent<Z>;

  // parse results from events and put them into the result
  result.args = schema.parse(event.args);
  result.blockHeight = event.block.height;

  return event as unknown as ValidatedEvent<Z>;
};
