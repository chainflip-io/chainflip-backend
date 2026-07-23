#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Submits a swap from one of our controlled external wallets and tracks everything that
// happens (events, rates, fees, chunks, external balance changes) into a structured JSON
// report. Designed to run against a live network, but works on a localnet too.
// The flow itself lives in shared/live/perform_live_swap.ts so live tests can reuse it.
//
// Runs an inline just-in-time LP "bot" (see shared/live/live_jit.ts)
// on our own LP account, so the swap fills against our own liquidity and the only loss is
// fees. Disable with --skipLpFill to execute against whatever liquidity the pool has (the
// fill-or-kill floor still bounds the price).
// The funds are returned to the swapping account once the swap is complete.
//
// Live usage (Perseverance):
//   export BOUNCER_NETWORK=perseverance
//   export CF_NODE_ENDPOINT=wss://archive.perseverance.chainflip.io
//   export ETH_ENDPOINT=<sepolia rpc>
//   export ETH_USDC_WHALE=<funded sepolia private key>
//   export ETH_USDC_ADDRESS=<sepolia usdc contract>
//   export BROKER_URI=<broker account secret uri>
//   export LP_URI=<lp account secret uri>
//   ./commands/live/submit_live_swap.ts Eth Usdc --amount 0.005
//
// Event queries go to the network's public indexer-gateway automatically (override with
// INDEXER_GATEWAY_URL)
//
// Localnet dry-run:
//   ./commands/live/submit_live_swap.ts Eth Usdc --amount 0.5

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { parseAssetString, runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger as logger } from 'shared/utils/logger';
import { networkTimeouts } from 'shared/live/live_config';
import { performLiveSwap } from 'shared/live/perform_live_swap';
import { writeReport } from 'shared/live/report';

interface Args {
  sourceAsset: string;
  destAsset: string;
  amount: string;
  destAddress?: string;
  toleranceBps?: number;
  refundDuration: number;
  dcaChunks?: number;
  chunkInterval?: number;
  registerBroker: boolean;
  skipLpFill: boolean;
  lpPrice?: number;
  registerLp: boolean;
  output?: string;
}

async function submitLiveSwap() {
  const args = yargs(hideBin(process.argv))
    .command('$0 <sourceAsset> <destAsset>', 'Submit a swap and produce a report', (a) =>
      a
        .positional('sourceAsset', { describe: 'Asset to swap from, e.g. Eth', type: 'string' })
        .positional('destAsset', { describe: 'Asset to swap to, e.g. Usdc', type: 'string' })
        .option('amount', {
          describe: 'Amount of the source asset to swap (human units)',
          type: 'string',
          demandOption: true,
        })
        .option('destAddress', {
          describe: 'Destination address (default: our own wallet)',
          type: 'string',
        })
        .option('toleranceBps', {
          describe: 'Fill-or-kill tolerance below the quoted rate, in bps',
          type: 'number',
        })
        .option('refundDuration', {
          describe: 'Fill-or-kill retry duration in state-chain blocks before refunding',
          type: 'number',
          default: 50,
        })
        .option('dcaChunks', {
          describe: 'DCA: number of chunks to split the swap into',
          type: 'number',
        })
        .option('chunkInterval', {
          describe: 'DCA: number of blocks between chunks',
          type: 'number',
          default: 2,
        })
        .option('registerBroker', {
          describe: 'Register the broker account if it has no role yet',
          type: 'boolean',
          default: false,
        })
        .option('skipLpFill', {
          describe: 'Do not fill the swap with our own LP liquidity',
          type: 'boolean',
          default: false,
        })
        .option('lpPrice', {
          describe:
            'Fixed LP order price (USDC per base asset) instead of one tick better than the pool',
          type: 'number',
        })
        .option('registerLp', {
          describe: 'Register the LP account if it has no role yet',
          type: 'boolean',
          default: false,
        })
        .option('output', {
          describe: 'Path of the JSON report (default: /tmp/chainflip/live_swap_<timestamp>.json)',
          type: 'string',
        }),
    )
    .help('h').argv as unknown as Args;

  const report = await performLiveSwap(logger, {
    sourceAsset: parseAssetString(args.sourceAsset),
    destAsset: parseAssetString(args.destAsset),
    amount: args.amount,
    destAddress: args.destAddress,
    toleranceBps: args.toleranceBps,
    refundDurationBlocks: args.refundDuration,
    dcaParams: args.dcaChunks
      ? { numberOfChunks: args.dcaChunks, chunkIntervalBlocks: args.chunkInterval! }
      : undefined,
    registerBroker: args.registerBroker,
    skipLpFill: args.skipLpFill,
    lpPrice: args.lpPrice,
    registerLp: args.registerLp,
  });

  const outputPath =
    args.output ?? `/tmp/chainflip/live_swap_${report.startedAt.replace(/[:.]/g, '-')}.json`;
  writeReport(logger, outputPath, report);
  logger.info(
    `Swap ${report.outcome}: sent ${report.amount} ${report.sourceAsset}, dest balance ` +
      `${report.externalBalances.dest.before} -> ${report.externalBalances.dest.after} ${report.destAsset}`,
  );

  if (report.outcome === 'incomplete') {
    throw new Error('Swap did not complete, see the report for details');
  }

  // Filling the swap with our own liquidity is the point of the run, and a swap can succeed on
  // somebody else's orders, so a JIT fill that bought nothing is a failure regardless of outcome.
  const unfilledLegs = report.lpFill?.unfilledLegs ?? [];
  if (unfilledLegs.length > 0) {
    throw new Error(
      `Our JIT orders did not fill the swap (${unfilledLegs
        .map((leg) => `${leg.side} ${leg.baseAsset}`)
        .join(', ')}); it was filled by other liquidity. See the report for details.`,
    );
  }
}

await runWithTimeoutAndExit(submitLiveSwap(), networkTimeouts().totalRunSeconds);
