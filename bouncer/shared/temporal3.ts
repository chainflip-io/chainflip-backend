#!/usr/bin/env -S pnpm tsx

import { z } from 'zod';
import { globalLogger } from './utils/logger';

// for exmple
import { InternalAsset as Asset } from '@chainflip/cli';
import { Event, getChainflipApi, observeEvent } from './utils/substrate';
import { amountToFineAmount, assetDecimals, ChainflipExtrinsicSubmitter, createStateChainKeypair, lpMutex, shortChainFromAsset, sleep } from './utils';
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

// const vaultActivated = z.object({__kind: z.literal('VaultActivated'), vault: z.string()});
// const IngressDetected = z.object({__kind: z.literal('IngressDetected'), volume: z.number()});

const boostFundsAdded = z.object({
    name: z.object({
        section: z.literal('lendingPools'),
        method: z.literal('BoostFundsAdded'),
    }),
    data: z.object({
        boosterId: z.string(),
        boostPool: z.object({
            asset: z.string(),
            tier: z.string()
        })
    })
})

const event = boostFundsAdded;
// z.object({
//     name: z.object({
//         section: z.string(),
//         method: z.string(),
//     }),
//     data: z.any()
// })


type CustomEvent = z.infer<typeof event>;

const broadcast = z.object({
    __kind: z.literal('Broadcast'),
    chain: z.string(),
    volume: z.number(),
})

const lendingPools = {
    addBoostFunds: z.object({
        __kind: z.literal('lendingPools:addBoostFunds'),
        chain: z.string(),
        amount: z.number(),
        boostTier: z.number(),
        lpUri: z.string(),
    })
}

const output = z.discriminatedUnion('__kind', [
    lendingPools.addBoostFunds,
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

    // const result: ChainflipT<number> = Then(vaultActivated, (val) => 
    //     Output(broadcast, {__kind: 'Broadcast', chain: val.vault, volume: 500},
    //         Then(IngressDetected, ({volume}) => Done(volume))
    //     )
    // );
}

interface Executor<N,I,O> {
    output(o: O): Promise<N>;
    input<Schema extends z.ZodType<I>>(startFrom: N, schema: Schema): Promise<[N, z.infer<Schema>]>;
    max(a: N, b: N): N;
}

interface Max<T> {
    max(a: T, b: T): T;
}


async function execute<N,I,O,X>(executor: Executor<N,I,O>, temporal: Temporal<I,O,X>, time: N): Promise<X> {
    if ('output' in temporal) {
        return temporal.output(async (schema, value, rest) => {
            const new_time = await executor.output(value);
            return execute(executor, rest, executor.max(time, new_time))
        });
    } else if ('input' in temporal) {
        return temporal.input(async (schema, next) => {
            const [new_time, input] = await executor.input(time, schema);
            return execute(executor, next(input), executor.max(time,new_time));
        });
    } else {
        return temporal.done;
    }
}

class BouncerExecutor implements Executor<number, z.infer<typeof event>, z.infer<typeof output>> {
    async output(o: Output): Promise<number> {
        const lp = createStateChainKeypair(o.lpUri);
        await using chainflip = await getChainflipApi();
        const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(o.lpUri));

        const result = await extrinsicSubmitter.submit(
            chainflip.tx.lendingPools.addBoostFunds(
            shortChainFromAsset('Btc').toUpperCase(),
            amountToFineAmount(o.amount.toString(), assetDecimals('Btc')),
            o.boostTier,
            ),
        );

        return (result as any).blockNumber.toNumber();
    }
    input<Schema extends z.ZodType<CustomEvent>>(startFrom: number, schema: Schema): Promise<[number, z.TypeOf<Schema>]> {
        return observeEvent(globalLogger, `:`, {
            test: (event) => true,
            schema: schema,
            temporalOptions: {
                startFrom: startFrom
            }
        }).event.then((value) => [value.block, value])
    }
    max(a: number, b: number): number {
        if (a > b) {
            return a;
        } else {
            return b;
        }
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


  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);
  const result = await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      amountToFineAmount(amount.toString(), assetDecimals(asset)),
      boostTier,
    ),
  );
  logger.info(`Extrinsic result is: ${JSON.stringify(result)}`);
  const blockHeight = (result as any).blockNumber.toNumber();
  logger.info(`Blockheight is ${blockHeight}... Sleeping`);

  const observeBoostFundsAdded = observeEvent(logger, `lendingPools:BoostFundsAdded`, {
    test: (event) => true,
    schema: boostFundsAdded.refine((event) => 
        event.data.boosterId === lp.address &&
        event.data.boostPool.asset === asset &&
        event.data.boostPool.tier === boostTier.toString(),
    ),
    temporalOptions: {
        startFrom: blockHeight
    }
  });

  const done = await observeBoostFundsAdded.event;

  logger.info("Success!");
  return done;
}


export async function addBoostFunds2(
//   logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri = '//LP_BOOST',
): Promise<string> {

    const lp = createStateChainKeypair(lpUri);

    const result: ChainflipT<string> =
    Output(lendingPools.addBoostFunds, {
        __kind: 'lendingPools:addBoostFunds',
        chain: 'Btc',
        amount: amount,
        boostTier: boostTier,
        lpUri 
    }, Then(boostFundsAdded.refine((event) => 
        event.data.boosterId === lp.address &&
        event.data.boostPool.asset === asset &&
        event.data.boostPool.tier === boostTier.toString(),
    ), (input) => Done(input.data.boostPool.tier)));

    const done = await execute(new BouncerExecutor(), result, 0);
    console.log(`tier: ${done}`);
    return "done";
}

// ----------- RUN ------------

await addBoostFunds2('Btc', 5, 0.1, '//LP_API')