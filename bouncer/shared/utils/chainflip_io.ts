import {
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  extractExtrinsicResult,
  lpMutex,
} from 'shared/utils';
import { z } from 'zod';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
import { DisposableApiPromise, getChainflipApi } from './substrate';
import { findEvent, findGoodOrBadEvent, findOneEventOfMany } from './indexer';

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

export type AccountType = 'Broker' | 'LP';

export type FullAccount<T extends AccountType> = {
  uri: `//${string}`;
  keypair: KeyringPair;
  type: T;
};

export type WithAccount<T extends AccountType> = { account: FullAccount<T> };
export type WithLpAccount = WithAccount<'LP'>;

export function fullAccountFromUri<A extends AccountType>(
  uri: `//${string}`,
  type: A,
): FullAccount<A> {
  return {
    uri,
    keypair: createStateChainKeypair(uri),
    type,
  };
}

export class ChainflipIO<Requirements> {
  /// The last block height at which either an input or an output operation happened,
  /// (that is either an extrinsic was submitted or an event was found)
  private lastIoBlockHeight: number;

  /// Methods can contain additional requirements that they have, that is additional
  /// data that should be available when calling them. For example, submitting an
  /// extrinsic requires a statechain account.
  readonly requirements: Requirements;

  constructor(requirements: Requirements) {
    this.lastIoBlockHeight = 0;
    this.requirements = requirements;
  }

  async submitExtrinsic<Data extends Requirements & { account: FullAccount<AccountType> }>(
    this: ChainflipIO<Data>,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    extrinsic: (api: DisposableApiPromise) => any,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
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

    // submit
    const result = extractExtrinsicResult(chainflip, await extrinsicSubmitter.submit(ext, false));
    if (result.ok) {
      console.log(`Successfully submitted`);
      this.lastIoBlockHeight = result.value.blockNumber.toNumber();
    } else {
      console.log(`Encountered error when submitting extrinsic: ${result.error}`);
    }
    return result;
  }

  async nextBlock() {
    this.lastIoBlockHeight += 1;
  }

  async forwardToEvent<Z extends z.ZodTypeAny = z.ZodTypeAny>(
    name: `${string}.${string}` | `.${string}`,
    schema: Z,
  ): Promise<z.infer<Z>> {
    const event = await findOneEventOfMany(
      { event: { name, schema } },
      {
        startFromBlock: this.lastIoBlockHeight,
      },
    );
    this.lastIoBlockHeight = event.blockHeight;
    return event.data;
  }

  async expectEventInSameBlock<Z extends z.ZodTypeAny = z.ZodTypeAny>(
    name: `${string}.${string}` | `.${string}`,
    schema: Z,
  ): Promise<z.infer<Z>> {
    const event = await findEvent(
      name,
      {
        startFromBlock: this.lastIoBlockHeight,
        endBeforeBlock: this.lastIoBlockHeight + 1,
      },
      schema,
    );
    this.lastIoBlockHeight = event.blockHeight;

    return event.args;
  }

  async forwardToEitherEvent<S extends Record<string, EventDescription>>(
    descriptions: S,
  ): Promise<ChooseSingleEvent<S>> {
    console.log(
      `waiting for either of the following events: ${JSON.stringify(Object.values(descriptions).map((d) => d.name))}`,
    );
    console.log(`starting from block ${this.lastIoBlockHeight}`);
    const event = await findOneEventOfMany(descriptions, {
      startFromBlock: this.lastIoBlockHeight,
    });
    this.lastIoBlockHeight = event.blockHeight;
    return event;
  }
}

// the following fixes the "TypeError: Do not know how to serialize a BigInt" error
declare global {
  interface BigInt {
    toJSON(): string;
  }
}
// eslint-disable-next-line no-extend-native, func-names
BigInt.prototype.toJSON = function () {
  return this.toString();
};

export type EventDescription = { name: string; schema: z.ZodTypeAny };

export type EventDescriptions = Record<string, EventDescription>;

export type ChooseSingleEvent<S extends Record<string, EventDescription>> = {
  [K in keyof S]: {
    key: K;
    data: z.infer<S[K]['schema']>;
    blockHeight: number;
  };
}[keyof S];

function chooseValue<S extends EventDescriptions>(schemas: S): ChooseSingleEvent<S> {
  // implementation chooses one schema at runtime
  const keys = Object.keys(schemas);
  const randomKey = keys[Math.floor(Math.random() * keys.length)] as keyof S;

  // parse something ...
  return schemas[randomKey].schema.parse(undefined as any);
}

function test() {
  const x = chooseValue({
    one: {
      name: `bla`,
      schema: z.number(),
    },
    other: {
      name: 'hello',
      schema: z.literal('hello'),
    },
  });

  if (x.key === 'one') {
    console.log(x.data);
  } else {
    console.log(x.data);
  }
}
