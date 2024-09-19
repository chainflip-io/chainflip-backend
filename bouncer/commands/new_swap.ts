#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Request a new swap with the provided parameters:
// <sourceAsset> <destAsset> <destAddress> [maxBoostFeeBps] [refundAddress] [minPrice] [refundDuration]
// Use `-h` for help.
// If the refundAddress is provided, the minPrice must also be provided. The refundDuration will default to 0 if not provided.
// Example: ./commands/new_swap.ts Dot Btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX --refundAddress "0xa0b52be60216f8e0f2eb5bd17fa3c66908cc1652f3080a90d3ab20b2d352b610" --minPrice 100000000000000000

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { InternalAsset } from '@chainflip/cli';
import { parseAssetString, runWithTimeoutAndExit } from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { RefundParameters } from '../shared/new_swap';

interface Args {
  sourceAsset: string;
  destAsset: string;
  destAddress: string;
  maxBoostFeeBps: number;
  refundAddress?: string;
  minPrice?: string;
  refundDuration: number;
}

async function newSwapCommand() {
  const args = yargs(hideBin(process.argv))
    .command(
      '$0 <sourceAsset> <destAsset> <destAddress> [maxBoostFeeBps] [refundAddress] [minPrice] [refundDuration]',
      'Request a new swap with the provided parameters',
      (a) => {
        console.log('Parsing options');
        return a
          .positional('sourceAsset', {
            describe: 'The source currency ("Btc", "Eth", "Dot", "Usdc")',
            type: 'string',
          })
          .positional('destAsset', {
            describe: 'The destination currency ("Btc", "Eth", "Dot", "Usdc")',
            type: 'string',
          })
          .positional('destAddress', {
            describe: 'The destination address',
            type: 'string',
          })
          .option('maxBoostFeeBps', {
            describe: 'The max boost fee bps (default: 0 (no boosting))',
            type: 'number',
            default: 0,
            demandOption: false,
          })
          .option('refundAddress', {
            describe: 'Fill or Kill refund address',
            type: 'string',
            demandOption: false,
          })
          .option('minPrice', {
            describe: 'Fill or Kill minimum price',
            type: 'string',
            demandOption: false,
          })
          .option('refundDuration', {
            describe: 'Fill or kill duration in blocks to retry the swap before refunding',
            type: 'number',
            demandOption: false,
            default: 0,
          });
      },
    )
    .help('h').argv as unknown as Args;

  if ((args.refundAddress === undefined) !== (args.minPrice === undefined)) {
    throw new Error('Must specify both refundAddress and minimumPrice when using refund options');
  }
  const refundParameters: RefundParameters | undefined =
    args.refundAddress !== undefined && args.minPrice !== undefined
      ? {
          retryDurationBlocks: args.refundDuration,
          refundAddress: args.refundAddress,
          minPrice: args.minPrice,
        }
      : undefined;

  await requestNewSwap(
    parseAssetString(args.sourceAsset) as InternalAsset,
    parseAssetString(args.destAsset) as InternalAsset,
    args.destAddress,
    undefined, // tag
    undefined, // messageMetadata
    undefined, // brokerCommissionBps
    true, // log
    args.maxBoostFeeBps,
    refundParameters,
  );
}

await runWithTimeoutAndExit(newSwapCommand(), 60);
