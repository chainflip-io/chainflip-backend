import prisma from 'client';
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

export const hexString = z
  .string()
  .refine((v): v is `0x${string}` => /^0x[0-9a-fA-F]*$/.test(v), { message: 'Invalid hex string' });

// export async function eventsFromCurrentBlock(): Promise<ChainflipIO> {
//   await using chainflip = await getChainflipApi();
//   const header = await chainflip.rpc.chain.getHeader();
//   return new ChainflipIO(header.number.toNumber());
// }

export type Ok<T> = { ok: true; value: T; unwrap: () => T };
export type Err<E> = { ok: false; error: E; unwrap: () => never };
export type Result<T, E> = Ok<T> | Err<E>;
export const Ok = <T>(value: T): Ok<T> => ({ ok: true, value, unwrap: () => value });
export const Err = <E>(error: E): Err<E> => ({
  ok: false,
  error,
  unwrap: () => {
    throw new Error(`${error}`);
  },
});

// ---------------------------------

export type AccountType = 'Broker' | 'Lp';

export type FullAccount<T extends AccountType> = {
  uri: `//${string}`;
  keypair: KeyringPair;
  type: T;
};

export type WithAccount<T extends AccountType> = { account: FullAccount<T> };
export type WithLpAccount = WithAccount<'Lp'>;

export class ChainflipIO<Requirements> {
  private lastIoBlockHeight: number;
  readonly requirements: Requirements;

  // state logging
  private actions: string[];

  constructor(requirements: Requirements) {
    this.lastIoBlockHeight = 0;
    this.requirements = requirements;
    this.actions = [];
  }

  async submitExtrinsic<Data extends Requirements & { account: FullAccount<AccountType> }>(
    this: ChainflipIO<Data>,
    extrinsic: (api: DisposableApiPromise) => any,
  ): Promise<Result<any, string>> {
    await using chainflip = await getChainflipApi();
    const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(
      this.requirements.account.keypair,
      lpMutex.for(this.requirements.account.uri),
    );
    const ext = extrinsic(chainflip);

    // generate readable description for logging
    const human = ext.toHuman();
    const section = human.method.section;
    const method = human.method.method;
    const args = human.method.args;
    const readable = `${section}.${method}(${JSON.stringify(args)})`;

    console.log(`Submitting extrinsic '${readable}' for ${this.requirements.account.uri}`);
    this.actions.push(`Submitting extrinsic '${readable}' for ${this.requirements.account.uri}`);

    // submit
    const result = extractExtrinsicResult(chainflip, await extrinsicSubmitter.submit(ext, false));
    if (result.ok) {
      console.log(`Successfully submitted`);
      this.actions.push(` => Done`);
      this.lastIoBlockHeight = result.value.blockNumber.toNumber();
    } else {
      console.log(`Encountered error when submitting extrinsic: ${result.error}`);
      this.actions.push(` => failed`);
    }
    return result;
  }

  async eventInSameBlock<Z extends z.ZodTypeAny = z.ZodTypeAny>(
    name: `${string}.${string}` | `.${string}`,
    schema: Z,
  ): Promise<z.infer<Z>> {
    this.actions.push(`Waiting for event ${name}`);
    const event = await findEvent(
      name,
      {
        startFromBlock: this.lastIoBlockHeight,
        endBeforeBlock: this.lastIoBlockHeight + 1,
      },
      {
        schema,
      },
    );
    this.lastIoBlockHeight = event.blockHeight;
    this.actions.push(`Done`);

    return event.args;
  }

  printActions() {
    for (const action of this.actions) {
      console.log(` - ${action}`);
    }
  }
}

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

  // return parsed args and replace
  event.args = schema.parse(event.args);
  event.height = event.block.height;

  return event as unknown as ValidatedEvent<Z>;
};
