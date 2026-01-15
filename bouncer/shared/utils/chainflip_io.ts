import {
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  extractExtrinsicResult,
  cfMutex,
  isValidHexHash,
} from 'shared/utils';
import { z } from 'zod';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
import { submitExistingGovernanceExtrinsic } from 'shared/cf_governance';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { governanceProposed } from 'generated/events/governance/proposed';
import { governanceExecuted } from 'generated/events/governance/executed';
import { DisposableApiPromise, getChainflipApi } from './substrate';
import {
  OneOfEventsResult,
  EventName,
  findOneEventOfMany,
  EventDescriptions,
  AllOfEventsResult,
  SingleEventResult,
  blockHeightOfTransactionHash,
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
   * Used by `this.runExclusively()` to ensure that this objects' async methods are always called sequentially.
   */
  private currentlyInUseBy: string | undefined;

  private currentStackTrace: string | undefined;

  /**
   * Creates a new instance, the `lastIoBlockHeight` has to be specified. If you want
   * to automatically initialize to the current block height, use `newChainflipIO` instead.
   */
  constructor(logger: Logger, requirements: Requirements, lastIoBlockHeight: number) {
    this.lastIoBlockHeight = lastIoBlockHeight;
    this.requirements = requirements;
    this.logger = logger;
    this.currentlyInUseBy = undefined;
    this.currentStackTrace = undefined;
  }

  private clone(): ChainflipIO<Requirements> {
    return new ChainflipIO(this.logger, this.requirements, this.lastIoBlockHeight);
  }

  withChildLogger(tag: string): ChainflipIO<Requirements> {
    return new ChainflipIO(this.logger.child({ tag }), this.requirements, this.lastIoBlockHeight);
  }

  with<Extension>(extension: Extension): ChainflipIO<Requirements & Extension> {
    return new ChainflipIO(
      this.logger,
      { ...this.requirements, ...extension },
      this.lastIoBlockHeight,
    );
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
   * @param arg.extrinsic Function that takes a `DisposableApiPromise` and builds the extrinsic that should be submitted.
   * @param arg.expectedEvent Optional event description containing `name` and optionally `schema`, describing the event
   * that's expected to be emitted during execution of the extrinsic
   * @returns The well-typed event data of the expected event if one was provided. Otherwise the full, untyped result object
   * that was returned by the extrinsic.
   */
  async submitExtrinsic<
    Data extends Requirements & { account: FullAccount<AccountType> },
    Schema extends z.ZodTypeAny,
  >(
    this: ChainflipIO<Data>,
    arg: {
      extrinsic: ExtrinsicFromApi;
      expectedEvent?: { name: EventName; schema?: Schema };
    },
  ): Promise<z.infer<Schema>> {
    return this.runExclusively('submitExtrinsic', async () => {
      await using chainflipApi = await getChainflipApi();
      const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(
        this.requirements.account.keypair,
        cfMutex.for(this.requirements.account.uri),
      );
      const ext = arg.extrinsic(chainflipApi);

      // generate readable description for logging
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const { section, method, args } = (ext as any).toHuman().method;
      const readable = `${section}.${method}(${JSON.stringify(args)})`;

      this.logger.debug(`Submitting extrinsic '${readable}' for ${this.requirements.account.uri}`);

      // submit
      const result = extractExtrinsicResult(
        chainflipApi,
        await extrinsicSubmitter.submit(ext, false),
      );
      if (!result.ok) {
        throw new Error(`'${readable}' failed (${result.error})`);
      }

      this.logger.debug(
        `Successfully submitted extrinsic with result ${JSON.stringify(result.value)}`,
      );
      this.lastIoBlockHeight = result.value.blockNumber.toNumber();

      // extract event data if expected
      if (arg.expectedEvent) {
        const txHash = `${result.value.txHash}`;
        this.logger.debug(
          `Searching for event ${arg.expectedEvent.name} caused by call to extrinsic ${readable} (tx hash: ${txHash})`,
        );
        const event = await findOneEventOfMany(
          this.logger,
          {
            event: {
              name: arg.expectedEvent.name,
              schema: arg.expectedEvent.schema ?? z.any(),
              txHash,
            },
          },
          {
            startFromBlock: this.lastIoBlockHeight,
            endBeforeBlock: this.lastIoBlockHeight + 1,
          },
        );
        this.logger.debug(
          `Found event ${arg.expectedEvent.name} caused by call to extrinsic ${readable}\nEvent data is: ${JSON.stringify(event)}`,
        );
        return event.data;
      }

      return result;
    });
  }

  /**
   * Submits a governance extrinsic and updates `lastIoBlockHeight` to the block were the extrinsic was included.
   * @param arg Object containing `extrinsic: (api: DisposableChainflipApi) => any` that should be submitted as governance proposal
   * and optionally an entry `expectedEvent` describing the event we expect to be emitted when the extrinsic is included.
   */
  submitGovernance = this.wrapWithExpectEvent((arg: { extrinsic: ExtrinsicFromApi }) =>
    this.impl_submitGovernance(arg),
  );

  private async impl_submitGovernance(arg: { extrinsic: ExtrinsicFromApi }): Promise<void> {
    // we only wrap the governance submission by `runExclusively`
    // because the second half invokes `stepUntilEvent` which has its own `runExclusively` wrapper.
    const proposalId = await this.runExclusively('submitGovernance', async () => {
      await using chainflipApi = await getChainflipApi();
      const extrinsic = await arg.extrinsic(chainflipApi);

      // generate readable description for logging
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const { section, method, args } = (extrinsic.toHuman() as any).method;
      const readable = `${section}.${method}(${JSON.stringify(args)})`;

      this.logger.debug(`Submitting governance extrinsic '${readable}' for snowwhite`);

      // TODO we might want to move this functionality here eventually
      return submitExistingGovernanceExtrinsic(extrinsic);
    });

    await this.stepUntilEvent(
      'Governance.Proposed',
      governanceProposed.refine((id) => id === proposalId),
    );
    this.logger.debug(
      `Governance proposal has id ${proposalId} and was found in block ${this.lastIoBlockHeight}`,
    );
    await this.stepUntilEvent(
      'Governance.Executed',
      governanceExecuted.refine((id) => id === proposalId),
    );
    this.logger.debug(
      `Governance proposal with id ${proposalId} executed in block ${this.lastIoBlockHeight}`,
    );
  }

  /**
   * Steps until it finds a block where the tx with the given hash was included.
   *
   * WARNING: this will loop indefinitely if the provided tx hash is never included.
   *
   * @param arg Object containing `hash: string` that references an on-chain transaction
   * and optionally an entry `expectedEvent` describing the event we expect to be emitted when the transaction is included.
   */
  stepToTransactionIncluded = this.wrapWithExpectEvent((arg: { hash: string }) =>
    this.impl_stepToTransactionIncluded(arg),
  );

  private async impl_stepToTransactionIncluded(arg: { hash: string }): Promise<void> {
    await this.runExclusively('stepToTransactionIncluded', async () => {
      if (!isValidHexHash(arg.hash)) {
        throw new Error(
          `Expected transaction hash but got ${arg.hash} when trying to step to tx included`,
        );
      }

      this.debug(`Waiting for block with transaction hash ${arg.hash}`);
      const height = await blockHeightOfTransactionHash(arg.hash);

      if (height >= this.lastIoBlockHeight) {
        this.debug(`Found transaction hash ${arg.hash} in block ${height}`);
        this.lastIoBlockHeight = height;
      } else {
        throw new Error(`When stepping to block with transaction with hash ${arg.hash}, found it in a block that's lower than the current IO height:
        - current lastIoBlockHeight: ${this.lastIoBlockHeight}
        - found tx in ${height}`);
      }
    });
  }

  /**
   * Runs `f` and afterwards tries to `expectEvent` if an event was provided.
   *
   * This function handles the common pattern of expecting a certain event after stepping to a block. So implementations
   * can be written without taking the expected event into account, then just have to wrapped with `wrapWithExpectEvent`.
   * @param f Method to be executed
   * @returns A function that's similar to `f` but additionally takes an `expectedEvent` parameter.
   */
  private wrapWithExpectEvent<A extends object>(
    f: (a: A) => Promise<void>,
  ): <Schema extends z.ZodTypeAny>(
    a: A & { expectedEvent?: { name: EventName; schema?: Schema } },
  ) => Promise<z.infer<Schema>> {
    return async (arg) => {
      await f(arg);
      if (arg.expectedEvent) {
        return this.expectEvent(arg.expectedEvent.name, arg.expectedEvent.schema ?? z.any());
      }
      return Promise.resolve();
    };
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
    return this.runExclusively('stepUntilEvent', async () => {
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
    });
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
    return this.runExclusively('expectEvent', async () => {
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
    });
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
    return this.runExclusively('stepUntilOneEventOf', async () => {
      this.logger.debug(
        `waiting for either of the following events: ${JSON.stringify(Object.values(descriptions).map((d) => d.name))} from block ${this.lastIoBlockHeight}`,
      );
      const event = await findOneEventOfMany(this.logger, descriptions, {
        startFromBlock: this.lastIoBlockHeight,
      });
      this.debug(`found event ${event}`);
      this.lastIoBlockHeight = event.blockHeight;
      return event;
    });
  }

  async stepUntilAllEventsOf<Events extends EventDescriptions>(
    events: Events,
  ): Promise<AllOfEventsResult<Events>> {
    // Note, this function is not wrapped in `runExclusively` because `this.all()` already is.
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
    return this.runExclusively('all', async () => {
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
    });
  }

  // --------------- api invariants ------------------

  /**
   * Makes sure that the function body `f` has exclusive access to this object while running.
   * @param method current method name (for better errors)
   * @param f the actual function body to run
   * @returns result of `f` is forwarded
   */
  private async runExclusively<A>(method: string, f: () => Promise<A>): Promise<A> {
    const stack = new Error().stack;
    if (this.currentlyInUseBy) {
      throw new Error(`Attempted to call a method on a cf object while it was already in use!

         - in use by '${this.currentlyInUseBy}'
         - tried to call on '${method}'

        This is not allowed, calls to the same cf object should be done strictly sequentially.

        If you want to run code in parallel, you should use the 'cf.all()' method to run "subtasks",
        e.g.: 'cf.all([cf => cf.method1(), cf => cf.method2()])'.

        The current lastIoBlockHeight is ${this.lastIoBlockHeight}.

        Current stack trace:
        ${stack}

        In use by stack trace:
        ${this.currentStackTrace}
        `);
    }
    this.currentlyInUseBy = method;
    this.currentStackTrace = stack;
    let result;
    try {
      result = await f();
    } finally {
      // always clean up even if we got an error
      this.currentlyInUseBy = undefined;
      this.currentStackTrace = undefined;
    }

    return result;
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
