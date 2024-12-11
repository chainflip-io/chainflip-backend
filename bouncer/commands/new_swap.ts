#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Request a new swap with the provided parameters:
// <sourceAsset> <destAsset> <destAddress> [maxBoostFeeBps] [refundAddress] [minPrice] [refundDuration] [numberOfChunks] [chunkInterval]
// Use `-h` for help.
// If the refundAddress is provided, the minPrice must also be provided. The minPrice is in source asset per dest asset. eg. 100 (= 100 Dot per Btc in the following example).
// The refundDuration is in blocks and will default to 0 if not provided.
// Example: ./commands/new_swap.ts Dot Btc n1ocq2FF95qopwbEsjUTy3ZrawwXDJ6UsX --refundAddress "0xa0b52be60216f8e0f2eb5bd17fa3c66908cc1652f3080a90d3ab20b2d352b610" --minPrice 100

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { InternalAsset } from '@chainflip/cli';
import {
  parseAssetString,
  runWithTimeoutAndExit,
  assetPriceToInternalAssetPrice,
  decodeDotAddressForContract,
  isPolkadotAsset,
} from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';

interface Args {
  sourceAsset: string;
  destAsset: string;
  destAddress: string;
  maxBoostFeeBps: number;
  refundAddress?: string;
  minPrice?: number;
  refundDuration: number;
  numberOfChunks?: number;
  chunkInterval?: number;
}

async function newSwapCommand() {
  const args = yargs(hideBin(process.argv))
    .command(
      '$0 <sourceAsset> <destAsset> <destAddress> [maxBoostFeeBps] [refundAddress] [minPrice] [refundDuration] [numberOfChunks] [chunkInterval]',
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
            describe: 'Fill or Kill minimum price in source asset per dest asset',
            type: 'number',
            demandOption: false,
          })
          .option('refundDuration', {
            describe: 'Fill or kill duration in blocks to retry the swap before refunding',
            type: 'number',
            demandOption: false,
            default: 0,
          })
          .option('numberOfChunks', {
            describe: 'DCA, number of chunks to split the swap into',
            type: 'number',
            demandOption: false,
          })
          .option('chunkInterval', {
            describe: 'DCA, number of blocks between each chunk',
            type: 'number',
            demandOption: false,
          });
      },
    )
    .help('h').argv as unknown as Args;

  // Fill or kill
  if ((args.refundAddress === undefined) !== (args.minPrice === undefined)) {
    throw new Error('Must specify both refundAddress and minimumPrice when using refund options');
  }
  const refundParameters: FillOrKillParamsX128 | undefined =
    args.refundAddress !== undefined && args.minPrice !== undefined
      ? {
          retryDurationBlocks: args.refundDuration,
          refundAddress: isPolkadotAsset(args.sourceAsset)
            ? decodeDotAddressForContract(args.refundAddress)
            : args.refundAddress,
          minPriceX128: assetPriceToInternalAssetPrice(
            args.sourceAsset as InternalAsset,
            args.destAsset as InternalAsset,
            args.minPrice,
          ),
        }
      : undefined;

  // DCA
  if ((args.numberOfChunks === undefined) !== (args.chunkInterval === undefined)) {
    throw new Error('Must specify both numberOfChunks and chunkInterval when using DCA');
  }
  const dcaParameters: DcaParams | undefined =
    args.numberOfChunks !== undefined && args.chunkInterval !== undefined
      ? {
          numberOfChunks: args.numberOfChunks,
          chunkIntervalBlocks: args.chunkInterval,
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
    dcaParameters,
  );
}

await runWithTimeoutAndExit(newSwapCommand(), 60);
