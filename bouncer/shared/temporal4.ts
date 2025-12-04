#!/usr/bin/env -S pnpm tsx

import { literal, z } from 'zod';
import { lendingPoolsBoostFundsAdded } from 'generated/events/lendingPools/boostFundsAdded';
import * as something from '@chainflip/processor';
import { globalLogger } from './utils/logger';

// for exmple
import { InternalAsset as Asset, Chain } from '@chainflip/cli';
import { Event, getChainflipApi, observeEvent } from './utils/substrate';
import {
  amountToFineAmount,
  assetDecimals,
  ChainflipExtrinsicSubmitter,
  createStateChainKeypair,
  lpMutex,
  shortChainFromAsset,
  sleep,
} from './utils';
import assert from 'assert';
import { ChainflipIO, findEvent, FullAccount, WithAccount } from './utils/indexer';
import { access } from 'fs';

// const findAwaitingActivationEvent = <Z extends z.ZodTypeAny>(chain: Chain, schema: Z) =>
//   findEvent(`${chain}Vault.AwaitingGovernanceActivation`, {
//     schema: z.object({ newPublicKey: schema }),
//   }).then((ev) => ev.args.newPublicKey!);

/// Adds existing funds to the boost pool of the given tier and returns the BoostFundsAdded event.
export async function addBoostFunds(
  //   logger: Logger,
  asset: Asset,
  boostTier: number,
  amount: number,
  lpUri: `//${string}` = '//LP_BOOST',
): Promise<string> {
  const logger = globalLogger;
  // await using chainflip = await getChainflipApi();
  // const lp = createStateChainKeypair(lpUri);
  // const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));
  const lp: FullAccount = {
    uri: lpUri,
    keypair: createStateChainKeypair(lpUri),
    type: 'Lp',
  };

  const chainflip: ChainflipIO<WithAccount> = new ChainflipIO({
    account: lp,
  });

  assert(boostTier > 0, 'Boost tier must be greater than 0');

  const fineAmount = amountToFineAmount(amount.toString(), assetDecimals(asset));

  // Add funds to the boost pool
  logger.debug(`Adding boost funds of ${amount} ${asset} at ${boostTier}bps`);

  await chainflip.submitExtrinsic((api) =>
    api.tx.lendingPools.addBoostFunds(
      shortChainFromAsset(asset).toUpperCase(),
      fineAmount,
      boostTier,
    ),
  );
  // logger.info(`Extrinsic result is: ${JSON.stringify(result)}`);
  // const blockHeight = (result as any).blockNumber.toNumber();
  // logger.info(`Blockheight is ${blockHeight}... Sleeping`);

  const event = await chainflip.eventInSameBlock(
    'LendingPools.BoostFundsAdded',
    lendingPoolsBoostFundsAdded.refine(
      (event) =>
        event.boostPool.tier === boostTier &&
        event.amount === BigInt(fineAmount) &&
        event.boosterId === lp.keypair.address,
    ),
  );

  logger.info(`Success! ${event.boosterId}`);
  return event.boosterId;
}

// import * as ss58 from '@chainflip/utils/ss58';
// const val = ss58.encode({ data: "0x2cab163688c64d90baaf1dbf036c9b7316dc47f3a41e3287b63c2a267f902b2b", ss58Format: 2112 })

// console.log(`encoded: ${val}`);

function zLiteralObject<T extends Record<string, string | number | boolean | object>>(
  obj: T,
): z.AnyZodObject {
  return z.object(
    Object.fromEntries(
      Object.entries(obj).map(([k, v]) => {
        if (typeof v === 'object') {
          return [k, zLiteralObject(v)];
        } else {
          return [k, z.literal(v)];
        }
      }),
    ) as { [K in keyof T]: z.ZodLiteral<T[K]> },
  );
}

type ZodObjectShape = Record<string, z.ZodTypeAny>;

function literalSubsetOf<
  S extends z.ZodObject<any>, // original Zod schema
  L extends Partial<z.infer<S>>, // subset of S, type-checked
>(schema: S, literalObject: L) {
  // Runtime validation: ensure every field in L conforms to S
  // const shape = schema.shape;
  // for (const key in literalObject) {
  //   shape[key].parse(literalObject[key]);   // throws at runtime if incompatible
  // }

  // Build literal schema
  const literalShape = Object.fromEntries(
    Object.entries(literalObject).map(([k, v]) => [k, z.literal(v)]),
  ) as {
    [K in keyof L]: z.ZodLiteral<L[K]>;
  };

  // return z.object(literalShape);

  return schema.merge(zLiteralObject(literalObject));
}

const schema = z.object({
  a: z.literal('hello'),
  b: z.number(),
});

// const bla = z.literal({a: 'hello'})

// const schema = zLiteralObject({
//   a: "hello",
//   b: z.number(),
// });

// schema.parse({
//   a: "bye",
//   b: 3,
// })

// literalSubsetOf(schema, {
//   b: 3,
//   a: "hello"
// })

addBoostFunds('Btc', 5, 0.1, '//LP_API');
