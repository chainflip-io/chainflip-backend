import {
  createStateChainKeypair,
  extractExtrinsicResult,
  cfMutex,
  isValidHexHash,
  sleep,
  waitForExt,
} from 'shared/utils';
import { z } from 'zod';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
import { submitExistingGovernanceExtrinsic } from 'shared/cf_governance';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { governanceProposed } from 'generated/events/governance/proposed';
import { governanceExecuted } from 'generated/events/governance/executed';
import { assertUnreachable } from '@polkadot/util';
import { DisposableApiPromise, getChainflipApi } from 'shared/utils/substrate';
import {
  OneOfEventsResult,
  EventName,
  findOneEventOfMany,
  EventDescriptions,
  AllOfEventsResult,
  blockHeightOfTransactionHash,
  highestBlock,
  EventFilter,
  EventDescription,
} from 'shared/utils/indexer';
import { Logger } from 'shared/utils/logger';
import { JsonValue } from 'generated/prisma/runtime/library';

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
   * Used to print logs with parallelism aware prefix
   */
  private getLoggingPrefix: (level: Severity) => string;

  /**
   * Creates a new instance, the `lastIoBlockHeight` has to be specified. If you want
   * to automatically initialize to the current block height, use `newChainflipIO` instead.
   */
  constructor(
    logger: Logger,
    requirements: Requirements,
    lastIoBlockHeight: number,
    getLoggingPrefix: (level: Severity) => string,
  ) {
    this.lastIoBlockHeight = lastIoBlockHeight;
    this.requirements = requirements;
    this.logger = logger;
    this.currentlyInUseBy = undefined;
    this.currentStackTrace = undefined;
    this.getLoggingPrefix = getLoggingPrefix;
  }

  private clone(): ChainflipIO<Requirements> {
    return new ChainflipIO(
      this.logger,
      this.requirements,
      this.lastIoBlockHeight,
      this.getLoggingPrefix,
    );
  }

  withChildLogger(tag: string): ChainflipIO<Requirements> {
    return new ChainflipIO(
      this.logger.child({ tag }),
      this.requirements,
      this.lastIoBlockHeight,
      this.getLoggingPrefix,
    );
  }

  with<Extension>(extension: Extension): ChainflipIO<Requirements & Extension> {
    return new ChainflipIO(
      this.logger,
      { ...this.requirements, ...extension },
      this.lastIoBlockHeight,
      this.getLoggingPrefix,
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
  submitExtrinsic = this.wrapWithExpectEvent<
    Requirements & WithAccount<AccountType>,
    { extrinsic: ExtrinsicFromApi }
  >(this.impl_submitExtrinsic);

  private async impl_submitExtrinsic<SubmissionRequirements extends WithAccount<AccountType>>(
    this: ChainflipIO<SubmissionRequirements>,
    arg: { extrinsic: ExtrinsicFromApi },
  ): Promise<EventFilter> {
    return this.runExclusively('submitExtrinsic', async () => {
      await using chainflipApi = await getChainflipApi();
      const ext = await arg.extrinsic(chainflipApi);

      // generate readable description for logging
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const { section, method, args } = (ext as any).toHuman().method;
      const readable = `${section}.${method}(${JSON.stringify(args)})`;

      this.debug(`Submitting extrinsic '${readable}' for ${this.requirements.account.uri}`);

      // submit
      const release = await cfMutex.acquire(this.requirements.account.uri);
      const { promise, waiter } = waitForExt(chainflipApi, this.logger, 'InBlock', release);
      const nonce = (await chainflipApi.rpc.system.accountNextIndex(
        this.requirements.account.keypair.address,
      )) as unknown as number;
      const unsub = await ext.signAndSend(this.requirements.account.keypair, { nonce }, waiter);
      const result = extractExtrinsicResult(chainflipApi, await promise);
      unsub();

      if (!result.ok) {
        throw new Error(`'${readable}' failed (${result.error})`);
      }

      this.debug(`Successfully submitted extrinsic with hash ${result.value.txHash}`);

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      this.lastIoBlockHeight = (result.value as any).blockNumber.toNumber();

      return {
        txHash: `${result.value.txHash}`,
      };
    });
  }

  /**
   * Submits an unsigned extrinsic and updates the `lastIoBlockHeight` to the block height were the extrinsic was included.
   * @param arg.extrinsic Function that takes a `DisposableApiPromise` and builds the extrinsic that should be submitted.
   * @param arg.expectedEvent Optional event description containing `name` and optionally `schema`, describing the event
   * that's expected to be emitted during execution of the extrinsic
   * @returns The well-typed event data of the expected event if one was provided. Otherwise the full, untyped result object
   * that was returned by the extrinsic.
   */
  submitUnsignedExtrinsic = this.wrapWithExpectEvent((arg: { extrinsic: ExtrinsicFromApi }) =>
    this.impl_submitUnsignedExtrinsic(arg),
  );

  private async impl_submitUnsignedExtrinsic(arg: {
    extrinsic: ExtrinsicFromApi;
  }): Promise<EventFilter> {
    return this.runExclusively('submitUnsignedExtrinsic', async () => {
      await using chainflipApi = await getChainflipApi();
      const extrinsic = await arg.extrinsic(chainflipApi);

      // generate readable description for logging
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const { section, method, args } = (extrinsic.toHuman() as any).method;
      const readable = `${section}.${method}(${JSON.stringify(args)})`;

      this.debug(`Submitting unsigned extrinsic '${readable}'`);

      const { promise, waiter } = waitForExt(chainflipApi, this.logger, 'InBlock');
      const unsub = await extrinsic.send(waiter);
      const result = extractExtrinsicResult(chainflipApi, await promise);
      unsub();

      if (!result.ok) {
        throw new Error(`'${readable}' failed (${result.error})`);
      }

      this.debug(`Successfully submitted unsigned extrinsic with hash ${result.value.txHash}`);

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      this.lastIoBlockHeight = (result.value as any).blockNumber.toNumber();

      return {
        txHash: `${result.value.txHash}`,
      };
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

  private async impl_submitGovernance(arg: { extrinsic: ExtrinsicFromApi }): Promise<EventFilter> {
    // we only wrap the governance submission by `runExclusively`
    // because the second half invokes `stepUntilEvent` which has its own `runExclusively` wrapper.
    const proposalId = await this.runExclusively('submitGovernance', async () => {
      await using chainflipApi = await getChainflipApi();
      const extrinsic = await arg.extrinsic(chainflipApi);

      // generate readable description for logging
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const { section, method, args } = (extrinsic.toHuman() as any).method;
      const readable = `${section}.${method}(${JSON.stringify(args)})`;

      this.debug(`Submitting governance extrinsic '${readable}' for snowwhite`);

      // TODO we might want to move this functionality here eventually
      return submitExistingGovernanceExtrinsic(extrinsic);
    });

    await this.stepUntilEvent(
      'Governance.Proposed',
      governanceProposed.refine((id) => id === proposalId),
    );
    this.debug(
      `Governance proposal has id ${proposalId} and was found in block ${this.lastIoBlockHeight}`,
    );
    await this.stepUntilEvent(
      'Governance.Executed',
      governanceExecuted.refine((id) => id === proposalId),
    );
    this.debug(
      `Governance proposal with id ${proposalId} executed in block ${this.lastIoBlockHeight}`,
    );

    // Since governance extrinsics are not executed in the extrinsic where they've been proposed,
    // we don't return a filter containing the txhash. We might consider a different filter here
    // in the future. Currently, when passing `expectedEvent`, it will accept any matching event
    // in the same block.
    return {};
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

  private async impl_stepToTransactionIncluded(arg: { hash: string }): Promise<EventFilter> {
    return this.runExclusively('stepToTransactionIncluded', async () => {
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

      return { txHash: arg.hash };
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
  // eslint-disable-next-line class-methods-use-this
  private wrapWithExpectEvent<R2, A extends object>(
    f: (this: ChainflipIO<R2>, a: A) => Promise<EventFilter>,
  ): <Schema extends z.ZodTypeAny>(
    this: ChainflipIO<R2>,
    a: A & { expectedEvent?: { name: EventName; schema?: Schema } },
  ) => Promise<z.infer<Schema>> {
    return async function wrapped(this: ChainflipIO<R2>, arg) {
      const eventFilter = await f.call(this, arg);
      if (arg.expectedEvent) {
        return this.expectEvent({
          name: arg.expectedEvent.name,
          schema: arg.expectedEvent.schema ?? z.any(),
          additionalFilter: eventFilter,
        });
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
      const event = await this.waitFor(
        `event ${name} from block ${this.lastIoBlockHeight}`,
        findOneEventOfMany(
          this.logger,
          { event: { name, schema } },
          {
            startFromBlock: this.lastIoBlockHeight,
          },
        ),
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
  async expectEvent<Schema extends z.ZodTypeAny>(
    description: EventDescription<Schema>,
  ): Promise<z.infer<Schema>> {
    return this.runExclusively('expectEvent', async () => {
      this.debug(`Expecting event ${description.name} in block ${this.lastIoBlockHeight}`);
      const result = await findOneEventOfMany(
        this.logger,
        { event: description },
        {
          startFromBlock: this.lastIoBlockHeight,
          endBeforeBlock: this.lastIoBlockHeight + 1,
        },
      );
      return result.data;
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
      let target;
      if (Object.values(descriptions).length > 1) {
        target = `either of the following events: ${JSON.stringify(Object.values(descriptions).map((d) => d.name))} from block ${this.lastIoBlockHeight}`;
      } else {
        target = `event ${JSON.stringify(Object.values(descriptions)[0].name)} from block ${this.lastIoBlockHeight}`;
      }
      const event = await this.waitFor(
        target,
        findOneEventOfMany(this.logger, descriptions, {
          startFromBlock: this.lastIoBlockHeight,
        }),
      );
      this.debug(`found event ${JSON.stringify(event)}`);
      this.lastIoBlockHeight = event.blockHeight;
      return event;
    });
  }

  async stepUntilAllEventsOf<Events extends EventDescriptions>(
    events: Events,
  ): Promise<AllOfEventsResult<Events>> {
    // Note, this function is not wrapped in `runExclusively` because `this.all()` already is.
    this.debug(
      `waiting for all of the following events: ${JSON.stringify(Object.values(events).map((d) => d.name))} from block ${this.lastIoBlockHeight}`,
    );
    const results = await this.all(
      Object.entries(events).map(
        ([key, event]) =>
          (cf) =>
            cf.stepUntilOneEventOf({ [key]: event }),
      ),
    );
    const merged: Record<string, { key: string; data: JsonValue; blockHeight: number }> =
      Object.assign({}, ...results.map((res) => ({ [res.key]: res })));

    this.debug(`got all the following event data: ${JSON.stringify(merged)}`);

    return merged as AllOfEventsResult<Events>;
  }

  // --------------- multi tasking support ------------------

  async all<T extends readonly ((cf: ChainflipIO<Requirements>) => unknown)[] | []>(
    values: T,
  ): Promise<{ -readonly [P in keyof T]: Awaited<ReturnType<T[P]>> }> {
    return this.runExclusively('all', async () => {
      this.info(`Starting tasks ${values.map((_, index) => index)}`);

      const n = values.length;

      // markers whether subtasks are still running
      const running: ('starting' | 'running' | 'success' | 'done')[] = Array(n).fill('starting');

      const getSymbol = (index: number, indexOfTalker: number) => {
        if (indexOfTalker === index) {
          switch (running[index]) {
            case 'starting':
              return '*';
            case 'success':
              return 'v';
            default:
              return '+';
          }
        }
        switch (running[index]) {
          case 'running':
            return '|';
          default:
            return ' ';
        }
      };

      // run all functions in parallel with clones of this chainflip io instance
      const results = await Promise.all(
        values.map(async (f, index) => {
          const cf = this.clone();
          const oldLoggingPrefix = cf.getLoggingPrefix;
          cf.getLoggingPrefix = (level: Severity) => {
            const taskState = Array.from({ length: n }, (_, i) => getSymbol(i, index)).join(' ');
            if (running[index] === 'starting') {
              running[index] = 'running';
            }
            return `${oldLoggingPrefix(level)} ${taskState} [${index}] `;
          };
          try {
            const result = await f(cf);
            running[index] = 'success';
            cf.debug(`Task ${index} finished successfully`);
            return { cf, result };
          } finally {
            running[index] = 'done';
          }
        }),
      );

      this.info(`All tasks ${values.map((_, index) => index)} finished successfully`);

      // collect all block heights and use the max height for our new block height
      this.lastIoBlockHeight = Math.max(...results.map((val) => val.cf.lastIoBlockHeight));

      // we have to typecast to the expected type
      return results.map((val) => val.result) as {
        -readonly [P in keyof T]: Awaited<ReturnType<T[P]>>;
      };
    });
  }

  // --------------- internal helpers ------------------
  private async waitFor<A>(target: string, promise: Promise<A>): Promise<A> {
    this.debug(`Waiting for ${target}`);

    let waiting = true;
    const messagePrinter = async () => {
      await sleep(30000);
      while (waiting) {
        this.debug(
          `Still waiting for ${target}. Current highest block height is ${await highestBlock()}.`,
        );
        await sleep(30000);
      }
    };

    // start printing messages, but don't await this promise
    // eslint-disable-next-line @typescript-eslint/no-floating-promises
    messagePrinter();

    const result = await promise;

    // stop printing messages
    waiting = false;

    return result;
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
    this.logger.trace(`${this.getLoggingPrefix('Trace')}${msg}`, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  debug(msg: string, ...args: any[]) {
    this.logger.debug(`${this.getLoggingPrefix('Debug')}${msg}`, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  info(msg: string, ...args: any[]) {
    this.logger.info(`${this.getLoggingPrefix('Info')}${msg}`, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  warn(msg: string, ...args: any[]) {
    this.logger.warn(`${this.getLoggingPrefix('Warn')}${msg}`, ...args);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  error(msg: string, ...args: any[]) {
    this.logger.error(`${this.getLoggingPrefix('Error')}${msg}`, ...args);
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
  return new ChainflipIO(logger, requirements, currentBlockHeight, (level: Severity) => {
    // make sure that the rest of the log is aligned by emiting whitespace for warn and info
    switch (level) {
      case 'Warn':
        return ' ';
      case 'Info':
        return ' ';
      case 'Error':
        return '';
      case 'Debug':
        return '';
      case 'Trace':
        return '';
      default:
        return assertUnreachable(level);
    }
  });
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

// ------------ Other ---------------
export type Severity = 'Error' | 'Warn' | 'Info' | 'Debug' | 'Trace';
