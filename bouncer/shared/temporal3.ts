#!/usr/bin/env -S pnpm tsx

import { z } from 'zod';
import { globalLogger } from './utils/logger';

// for exmple
import { InternalAsset as Asset } from '@chainflip/cli';
import { Event, getChainflipApi, observeEvent } from './utils/substrate';
import { amountToFineAmount, assetDecimals, ChainflipExtrinsicSubmitter, createStateChainKeypair, lpMutex, shortChainFromAsset } from './utils';
import assert from 'assert';

// type Cont<T> = <A>(cont: (t: T) => A) => A;

type Temporal<I,O,X> = {
    input: <A>(cont: <T extends z.ZodType<I>>(is: T, run: (i: z.infer<T>) => Temporal<I,O,X>) => A) => A
} | {
    output: <A>(cont: <T extends z.ZodType<O>>(s: T, output: z.infer<T>, rest: Temporal<I,O,X>) => A) => A,
} | {
    done: X
}

const Done = <I,O,X>(x: X): Temporal<I,O,X> => ({ done: x });

const Then = <I,O,X, T extends z.ZodType<I>>(si: T, run: (i: z.infer<T>) => Temporal<I,O,X>): Temporal<I,O,X> => ({
    input: (cont) => cont(si, run)
});

const Output = <I,O,X, T extends z.ZodType<O>>(s: T, value: z.infer<T>, rest: Temporal<I,O,X>): Temporal<I,O,X> => (
    {output: (cont) => cont(s, value, rest)}
)

// const map = <X, Y>(f: (x: X) => Y, t: Temporal<X>): Temporal<Y> => {
//     if ('then' in t) {
//         // return t.then((is, run) => ({
//         //     then: (cont) => cont(is, (i) => map(f, run(i)))
//         // }))
//     } else {
//         return {
//             done: f(t.done)
//         }
//     }
// };

// type Event = {
//     __kind: 'VaultActivated',
//     vault: string,
// } | {
//     __kind: 'IngressDetected',
//     volume: number,
// };

const vaultActivated = z.object({__kind: z.literal('VaultActivated'), vault: z.string()});
const IngressDetected = z.object({__kind: z.literal('IngressDetected'), volume: z.number()});

const event = z.discriminatedUnion('__kind', [
    vaultActivated,
    IngressDetected,
])

type CustomEvent = z.infer<typeof event>;

const broadcast = z.object({
    __kind: z.literal('Broadcast'),
    chain: z.string(),
    volume: z.number(),
})

const output = z.discriminatedUnion('__kind', [
    broadcast,
])

type Output = z.infer<typeof output>;

// type Output = {
//     __kind: 'Broadcast',
//     chain: string,
//     volume: number
// }

type ChainflipT<X> = Temporal<z.infer<typeof event>, z.infer<typeof output>, X>;

function test() {
    const schema = z.object({
        val1: z.number(),
        val2: z.boolean(),
    });

    const result: ChainflipT<number> = Then(vaultActivated, (val) => 
        Output(broadcast, {__kind: 'Broadcast', chain: val.vault, volume: 500},
            Then(IngressDetected, ({volume}) => Done(volume))
        )
    );
}

interface Executor<N,I,O> {
    output(o: O): Promise<N>;
    input<Schema extends z.ZodType<I>>(startFrom: N, schema: Schema): Promise<[N, z.infer<Schema>]>;
}

interface Max {
    max(other: this): this;
}

async function execute<N extends Max,I,O,X>(executor: Executor<N,I,O>, temporal: Temporal<I,O,X>, time: N): Promise<X> {
    if ('output' in temporal) {
        return temporal.output(async (schema, value, rest) => {
            const new_time = await executor.output(value);
            return execute(executor, rest, time.max(new_time))
        });
    } else if ('input' in temporal) {
        return temporal.input(async (schema, next) => {
            const [new_time, input] = await executor.input(time, schema);
            return execute(executor, next(input), time.max(new_time));
        });
    } else {
        return temporal.done;
    }
}

class BouncerExecutor implements Executor<number, z.infer<typeof event>, z.infer<typeof output>> {
    output(o: Output): Promise<number> {
        throw new Error('Method not implemented.');
    }
    input<Schema extends z.ZodType<CustomEvent>>(startFrom: number, schema: Schema): Promise<[number, z.TypeOf<Schema>]> {
        throw new Error('Method not implemented.');
    }

}

/// Adds existing funds to the boost pool of the given tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
//   logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri = '//LP_BOOST',
): Promise<Event> {
  const logger = globalLogger;
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const observeBoostFundsAdded = observeEvent(logger, `lendingPools:BoostFundsAdded`, {
    test: (event) =>
      event.data.boosterId === lp.address &&
      event.data.boostPool.asset === asset &&
      event.data.boostPool.tier === boostTier.toString(),
  });

  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );

  return observeBoostFundsAdded.event;
}


export async function addBoostFunds2(
//   logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri = '//LP_BOOST',
): Promise<Event> {
  const logger = globalLogger;
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const observeBoostFundsAdded = observeEvent(logger, `lendingPools:BoostFundsAdded`, {
    test: (event) =>
      event.data.boosterId === lp.address &&
      event.data.boostPool.asset === asset &&
      event.data.boostPool.tier === boostTier.toString(),
  });

  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );

  return observeBoostFundsAdded.event;
}

// ----------- RUN ------------

await addBoostFunds('Btc', 5, 0.1, '//LP_API')