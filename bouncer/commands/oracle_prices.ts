#!/usr/bin/env -S pnpm tsx
// Read-only fetch of the Chainlink oracle prices the state chain currently holds, decoded to USD.
// Calls the `cf_oracle_prices` custom RPC and converts each raw fixed-point price into a
// human-readable "USD per <asset>" value. Nothing is submitted.
//
// Usage (from bouncer/):
//   ./commands/oracle_prices.ts                        # all prices, localnet
//   ./commands/oracle_prices.ts --network mainnet      # all prices on mainnet
//   ./commands/oracle_prices.ts --network mainnet Eth  # just Eth (filter by base asset)
//   ./commands/oracle_prices.ts --network mainnet --json   # raw decoded rows as JSON (pipeable)
//
// Target network: defaults to localnet (ws://127.0.0.1:9944). Being read-only, it is safe against
// any network including mainnet. Point it elsewhere with (precedence top to bottom):
//   --endpoint <ws(s)-url>                                         any custom endpoint
//   --network <mainnet|berghain|perseverance|sisyphos|localnet>    known public endpoints
//   CF_NODE_ENDPOINT=<ws(s)-url>                                   env var (repo-wide convention)
//
// Price encoding: `cf_oracle_prices` returns `price` as a hex `cf_amm_math::Price` — a Q128.128
// fixed-point ratio of quote-fine-units per base-fine-units. USD-per-asset is therefore
//   price / 2^128 * 10^(base_decimals - quote_decimals)
// (decimals per PriceAsset, mirroring state-chain/.../oracle_price/price.rs).

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { Asset, assetDecimals, runWithTimeoutAndExit } from 'shared/utils';
import { resolveHttpEndpoint, withNetworkOptions } from 'shared/utils/networks';
import { jsonRpc } from 'shared/json_rpc';
import { globalLogger } from 'shared/utils/logger';

function priceAssetDecimals(priceAsset: string): number {
  if (priceAsset === 'Usd') return 6;
  return assetDecimals(priceAsset as Asset);
}

interface RawOraclePrice {
  price: string; // hex-encoded Q128.128 fixed point
  updated_at_oracle_timestamp: number;
  updated_at_statechain_block: number;
  base_asset: string;
  quote_asset: string;
  price_status: string;
}

// USD-per-base, computed in fixed point to avoid float precision loss. Returns a decimal string.
// value = price / 2^128 * 10^(baseDec - quoteDec); we carry `DISPLAY` extra decimals then format.
const DISPLAY = 8; // Chainlink's native precision
function decodePriceToUsd(raw: RawOraclePrice): string {
  const price = BigInt(raw.price);
  const baseDec = priceAssetDecimals(raw.base_asset);
  const quoteDec = priceAssetDecimals(raw.quote_asset);
  const exp = baseDec - quoteDec; // >= 0 for all current pairs
  const numer = price * 10n ** BigInt(Math.max(exp, 0) + DISPLAY);
  const denom = 2n ** 128n * 10n ** BigInt(Math.max(-exp, 0));
  const scaled = numer / denom; // = usdPrice * 10^DISPLAY
  const intPart = scaled / 10n ** BigInt(DISPLAY);
  const fracPart = (scaled % 10n ** BigInt(DISPLAY)).toString().padStart(DISPLAY, '0');
  return `${intPart}.${fracPart}`.replace(/\.?0+$/, '') || '0';
}

function ago(unixSeconds: number): string {
  const secs = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds);
  if (secs < 90) return `${secs}s ago`;
  if (secs < 5400) return `${Math.round(secs / 60)}m ago`;
  return `${Math.round(secs / 3600)}h ago`;
}

async function main() {
  const argv = await withNetworkOptions(
    yargs(hideBin(process.argv)).usage(
      '$0 [baseAsset] [options] — fetch the state chain oracle prices, decoded to USD',
    ),
  )
    .option('json', { type: 'boolean', default: false, describe: 'Output decoded rows as JSON' })
    .strictOptions()
    .help().argv;

  const httpEndpoint = resolveHttpEndpoint({ endpoint: argv.endpoint, network: argv.network });
  console.error(`Querying oracle prices from ${httpEndpoint}`);

  // Fetch all (base_and_quote_asset = None); filter by base asset client-side if one was given.
  const all = (await jsonRpc(globalLogger, 'cf_oracle_prices', [null], httpEndpoint)) as unknown as
    | RawOraclePrice[]
    | null;
  const baseFilter = argv._[0] !== undefined ? String(argv._[0]).toLowerCase() : undefined;
  const rows = (all ?? [])
    .filter((r) => !baseFilter || r.base_asset.toLowerCase() === baseFilter)
    .map((r) => ({
      pair: `${r.base_asset}/${r.quote_asset}`,
      usd: decodePriceToUsd(r),
      rawPrice: r.price,
      status: r.price_status,
      updated: ago(r.updated_at_oracle_timestamp),
      block: r.updated_at_statechain_block,
    }));

  if (rows.length === 0) {
    console.error(
      baseFilter ? `No oracle price for base asset '${baseFilter}'.` : 'No oracle prices returned.',
    );
    return;
  }

  if (argv.json) {
    console.log(JSON.stringify(rows, null, 2));
    return;
  }

  const pairW = Math.max(4, ...rows.map((r) => r.pair.length));
  const usdW = Math.max(9, ...rows.map((r) => r.usd.length));
  const statusW = Math.max(6, ...rows.map((r) => r.status.length));
  console.log(
    `${'PAIR'.padEnd(pairW)}  ${'USD'.padStart(usdW)}  ${'STATUS'.padEnd(statusW)}  UPDATED (BLOCK)`,
  );
  for (const r of rows) {
    console.log(
      `${r.pair.padEnd(pairW)}  ${r.usd.padStart(usdW)}  ${r.status.padEnd(statusW)}  ${r.updated} (#${r.block})`,
    );
  }
}

await runWithTimeoutAndExit(main(), 60, false);
