import {
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  extractExtrinsicResult,
  cfMutex,
} from 'shared/utils';
import { z } from 'zod';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
import { submitExistingGovernanceExtrinsic } from 'shared/cf_governance';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { governanceProposed } from 'generated/events/governance/proposed';
import { DisposableApiPromise, getChainflipApi } from './substrate';
import {
  OneOfEventsResult,
  EventName,
  findOneEventOfMany,
  EventDescriptions,
  AllOfEventsResult,
  SingleEventResult,
} from './indexer';
import { Logger } from './logger';

export class ChainflipIO<Requirements> {
  /**
   * The last block height at which either an input or an output operation happened,
   * (that is either an extrinsic was submitted or an event was found)
   */
  private lastIoBlockHeight: number;

  /**
   * Methods can contain additional requirements that they have, that is, additional
   * data that should be available when calling them. For example, submitting an
   * extrinsic requires a statechain account.
   */
  readonly requirements: Requirements;

  /** This class also exposes logger functionality. */
  readonly logger: Logger;

  /**
   * Creates a new instance, the `lastIoBlockHeight` has to be specified. If you want
   * to automatically initialize to the current block height, use `newChainflipIO` instead.
   */
  constructor(logger: Logger, requirements: Requirements, lastIoBlockHeight: number) {
    this.lastIoBlockHeight = lastIoBlockHeight;
    this.requirements = requirements;
    this.logger = logger;
  }

  private clone(): ChainflipIO<Requirements> {
    return new ChainflipIO(this.logger, this.requirements, this.lastIoBlockHeight);
  }

  withChildLogger(tag: string): ChainflipIO<Requirements> {
    return new ChainflipIO(this.logger.child({ tag }), this.requirements, this.lastIoBlockHeight);
  }

  ifYouCallThisYouHaveToRefactor_stepToBlockHeight(newIoBlockHeight: number) {
    if (this.lastIoBlockHeight > newIoBlockHeight) {
      throw new Error(
        'Error in ChainflipIO: `stepToBlockHeight` called with lower block height than current',
      );
    }
    this.lastIoBlockHeight = newIoBlockHeight;
  }

  /**
   * Submits an extrinsic and updates the `lastIoBlockHeight` to the block height were the extrinsic was included.
   * @param this Automatically provided by typescript when called as a method on a ChainflipIO object.
   * @param extrinsic Function that takes a `DisposableApiPromise` and builds the extrinsic that should be submitted.
   * @returns The result of submitting the extrinsic if successful, or a string containing the failure reason.
   */
  async stepToExtrinsicIncluded<Data extends Requirements & { account: FullAccount<AccountType> }>(
    this: ChainflipIO<Data>,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    extrinsic: (api: DisposableApiPromise) => any,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ): Promise<Result<any, string>> {
    await using chainflipApi = await getChainflipApi();
    const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(
      this.requirements.account.keypair,
      cfMutex.for(this.requirements.account.uri),
    );
    const ext = extrinsic(chainflipApi);

    // generate readable description for logging
    const { section, method, args } = ext.toHuman().method;
    const readable = `${section}.${method}(${JSON.stringify(args)})`;

    this.logger.debug(`Submitting extrinsic '${readable}' for ${this.requirements.account.uri}`);

    // submit
    const result = extractExtrinsicResult(
      chainflipApi,
      await extrinsicSubmitter.submit(ext, false),
    );
    if (result.ok) {
      this.logger.debug(`Successfully submitted`);
      this.lastIoBlockHeight = result.value.blockNumber.toNumber();
    } else {
      this.logger.debug(`Encountered error when submitting extrinsic: ${result.error}`);
    }
    return result;
  }

  /**
   * Submits a governance extrinsic and updates `lastIoBlockHeight` to the block were the extrinsic was included.
   * @param arg Object containing `extrinsic: (api: DisposableChainflipApi) => any` that should be submitted as governance proposal
   * and optionally an entry `expectedEvent` describing the event we expect to be emitted when the extrinsic is included.
   */
  async submitGovernance(arg: { extrinsic: ExtrinsicFromApi }): Promise<number>;
  async submitGovernance(arg: {
    extrinsic: ExtrinsicFromApi;
    expectedEvent: { name: EventName };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  }): Promise<SingleEventResult<'event', any>>;
  async submitGovernance<EventSchema extends z.ZodTypeAny>(arg: {
    extrinsic: ExtrinsicFromApi;
    expectedEvent: {
      name: EventName;
      schema: EventSchema;
    };
  }): Promise<SingleEventResult<'event', EventSchema>>;
  async submitGovernance<Schema extends z.ZodTypeAny>(arg: {
    extrinsic: ExtrinsicFromApi;
    expectedEvent?: {
      name: EventName;
      schema?: Schema;
    };
  }) {
    await using chainflipApi = await getChainflipApi();
    const extrinsic = await arg.extrinsic(chainflipApi);

    // generate readable description for logging
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const { section, method, args } = (extrinsic.toHuman() as any).method;
    const readable = `${section}.${method}(${JSON.stringify(args)})`;

    this.logger.debug(`Submitting governance extrinsic '${readable}' for snowwhite`);

    // TODO we might want to move this functionality here eventually
    const proposalId = await submitExistingGovernanceExtrinsic(extrinsic);
    await this.stepUntilEvent(
      'Governance.Proposed',
      governanceProposed.refine((id) => id === proposalId),
    );
    this.logger.debug(
      `Governance proposal has id ${proposalId} and was found in block ${this.lastIoBlockHeight}`,
    );

    // searching for event
    if (arg.expectedEvent) {
      const result = await this.stepUntilEvent(
        arg.expectedEvent.name,
        arg.expectedEvent.schema ?? z.any(),
      );
      return result;
    }
    return proposalId;
  }

  /**
   * Advance the current chainflip block height by one block.
   */
  async stepOneBlock() {
    this.lastIoBlockHeight += 1;
  }

  /**
   * Advance the current chainflip block height until an event
   * is found that matches the provided name and schema.
   * @param name Name of the event to search for. Can be provided with, or without pallet name.
   * @param schema Expected zod schema that the event data should match. This describes both
   * the shape of the data (e.g. which fields of which types exist), but can also require
   * various to fields to have specific values (e.g. ChannelId should have a certain expected value).
   * @returns The data of the first matching event, well-typed according to the provided schema.
   */
  async stepUntilEvent<Z extends z.ZodTypeAny = z.ZodTypeAny>(
    name: EventName,
    schema: Z,
  ): Promise<z.infer<Z>> {
    this.logger.debug(`waiting for event ${name} from block ${this.lastIoBlockHeight}`);
    const event = await findOneEventOfMany(
      this.logger,
      { event: { name, schema } },
      {
        startFromBlock: this.lastIoBlockHeight,
      },
    );
    this.lastIoBlockHeight = event.blockHeight;
    return event.data;
  }

  /**
   * Find event with the provided name and schema in the current chainflip block. This method
   * does not update the current chainflip block height.
   * @param name Name of the event to search for. Can be provided with, or without pallet name.
   * @param schema Expected zod schema that the event data should match. This describes both
   * the shape of the data (e.g. which fields of which types exist), but can also require
   * various to fields to have specific values (e.g. ChannelId should have a certain expected value).
   * @returns The data of the first matching event, well-typed according to the provided schema.
   */
  async expectEvent<Z extends z.ZodTypeAny = z.ZodTypeAny>(
    name: EventName,
    schema?: Z,
  ): Promise<z.infer<Z>> {
    this.logger.debug(`Expecting event ${name} in block ${this.lastIoBlockHeight}`);
    const event = await findOneEventOfMany(
      this.logger,
      { event: { name, schema: schema ?? z.any() } },
      {
        startFromBlock: this.lastIoBlockHeight,
        endBeforeBlock: this.lastIoBlockHeight + 1,
      },
    );

    return event.data;
  }

  /**
   * Advance the current chainflip block height until an event that matches one of the given
   * event descriptions (name and schema).
   * @param descriptions Record containing an arbitrary number of event descriptions (name and schema).
   * @returns Object containing the key and data of the found event, as well as the block height at which
   * it was found.
   */
  async stepUntilOneEventOf<Events extends EventDescriptions>(
    descriptions: Events,
  ): Promise<OneOfEventsResult<Events>> {
    this.logger.debug(
      `waiting for either of the following events: ${JSON.stringify(Object.values(descriptions).map((d) => d.name))} from block ${this.lastIoBlockHeight}`,
    );
    const event = await findOneEventOfMany(this.logger, descriptions, {
      startFromBlock: this.lastIoBlockHeight,
    });
    this.debug(`found event ${event}`);
    this.lastIoBlockHeight = event.blockHeight;
    return event;
  }

  async stepUntilAllEventsOf<Events extends EventDescriptions>(
    events: Events,
  ): Promise<AllOfEventsResult<Events>> {
    this.logger.debug(
      `waiting for all of the following events: ${JSON.stringify(Object.values(events).map((d) => d.name))} from block ${this.lastIoBlockHeight}`,
    );
    const results = await this.all(
      Object.entries(events).map(
        ([key, event]) =>
          (cf) =>
            cf.stepUntilOneEventOf({ [key]: event }),
      ),
    );
    const merged: Record<string, SingleEventResult<string, z.ZodTypeAny>> = Object.assign(
      {},
      ...results.map((res) => ({ [res.key]: res })),
    );

    this.logger.debug(`got all the following event data: ${JSON.stringify(merged)}`);

    return merged as AllOfEventsResult<Events>;
  }

  async all<T extends readonly ((cf: ChainflipIO<Requirements>) => unknown)[] | []>(
    values: T,
  ): Promise<{ -readonly [P in keyof T]: Awaited<ReturnType<T[P]>> }> {
    // run all functions in parallel with clones of this chainflip io instance
    const results = await Promise.all(
      values.map(async (f) => {
        const cf = this.clone();
        const result = await f(cf);
        return { cf, result };
      }),
    );

    // collect all block heights and use the max height for our new block height
    this.lastIoBlockHeight = Math.max(...results.map((val) => val.cf.lastIoBlockHeight));

    // we have to typecast to the expected type
    return results.map((val) => val.result) as {
      -readonly [P in keyof T]: Awaited<ReturnType<T[P]>>;
    };
  }

  // --------------- logger functionality ------------------

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  trace(msg: string, ...args: any[]) {
    this.logger.trace(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  debug(msg: string, ...args: any[]) {
    this.logger.debug(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  info(msg: string, ...args: any[]) {
    this.logger.info(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  warn(msg: string, ...args: any[]) {
    this.logger.warn(msg, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error(msg: string, ...args: any[]) {
    this.logger.error(msg, ...args);
  }
}

/**
 * Constructor for `ChainflipIO` objects. This initializes the internally tracked
 * block height to the latest height. To do this, it has to use async and thus cannot
 * be part of the official constructor.
 * @param logger Logger object that should be used for the exposed logging functionality.
 * @param requirements Possibly required additional data. This depends on which methods
 * are going to be called on the ChainflipIO object. Its type `Requirements` should
 * be automatically inferred and guide you to provide the correct fields.
 * @returns Newly initialized ChainflipIO object.
 */
export async function newChainflipIO<Requirements>(logger: Logger, requirements: Requirements) {
  // find out current block height
  await using chainflipApi = await getChainflipApi();
  const currentBlockHeight = (await chainflipApi.rpc.chain.getHeader()).number.toNumber();

  // initialize with this height, meaning that we'll only search for events from this height on
  return new ChainflipIO(logger, requirements, currentBlockHeight);
}

// ------------ Extrinsic types  ---------------
export type ExtrinsicFromApi = (
  api: DisposableApiPromise,
) => SubmittableExtrinsic<'promise'> | Promise<SubmittableExtrinsic<'promise'>>;
// ------------ Account types  ---------------

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

// ------------ Result type ---------------

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
