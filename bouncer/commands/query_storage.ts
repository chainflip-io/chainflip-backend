#!/usr/bin/env -S pnpm tsx
// Generic read-only storage query against the running state chain via dedot. Reads any pallet
// storage entry (plain value, map, or double/n-map) and prints the decoded value as JSON. Nothing
// is submitted.
//
// Usage (from bouncer/):
//   ./commands/query_storage.ts                              # list pallets that have storage
//   ./commands/query_storage.ts swapping                     # list swapping's storage entries
//   ./commands/query_storage.ts --search loan                # find entries across all pallets (name + docs)
//   ./commands/query_storage.ts swapping networkFeeForAsset Btc   # map entry with a full key -> exact value
//   ./commands/query_storage.ts swapping networkFeeForAsset       # map with no key -> dump ALL entries
//   ./commands/query_storage.ts swapping collectedNetworkFee      # plain value -> read it
//   ./commands/query_storage.ts <pallet> <entry> <partialKey>     # n-map with a partial key -> prefix dump
//
// How many keys to pass is inferred from the entry's metadata (no flag needed): pass the full key
// set for an exact lookup, fewer keys (or none) to dump all matching entries. A plain StorageValue
// takes no keys — if it holds a map, the read returns the whole collection.
//
// `--search <term>` (alias `--find`) is how to locate an item without knowing its pallet: it does a
// case-insensitive substring match over every `pallet.entry` name and its docs, printing the
// matching `{ pallet, entry, docs }` pairs to feed back in as a query. Note it matches storage
// entry NAMES, not nested struct fields — e.g. searching "minimumLoan" finds nothing, but "lending"
// surfaces `lendingConfig`, whose decoded value contains `minimumLoanAmountUsd`.
//
// Pallet and entry names are the camelCase dedot keys (e.g. "swapping", "networkFeeForAsset").
// Keys are parsed as JSON when possible (numbers, objects, arrays), otherwise treated as a string;
// so `Btc` is the string "Btc", `5` is the number 5, `{"chain":"Bitcoin"}` is an object. Large
// integers should be passed as quoted decimal strings — they are decoded to BigInt.
//
// Target network: defaults to localnet (ws://127.0.0.1:9944). Being read-only, it is safe to run
// against any network including mainnet. Point it elsewhere with (precedence top to bottom):
//   --endpoint <wss-url>                                           any custom endpoint
//   --network <mainnet|berghain|perseverance|sisyphos|localnet>    known public endpoints
//   CF_NODE_ENDPOINT=<wss-url>                                     env var (repo-wide convention)
// Flags may appear anywhere in the args. Examples:
//   ./commands/query_storage.ts --network mainnet swapping networkFeeForAsset Btc
//   ./commands/query_storage.ts lendingPools lendingConfig --endpoint wss://perseverance.chainflip.xyz

import { DedotClient, WsProvider as DedotWsProvider } from 'dedot';
import type { ChainflipNodeApi } from 'generated/chaintypes/chainflip-node';
import { getChainflipApi, type DisposableChainflipClient } from 'shared/utils/substrate';
import type { ChainflipClient } from 'shared/utils/dedot';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import {
  bigintReplacer,
  bigintReviver,
  lowercaseFirstLetter,
  runWithTimeoutAndExit,
} from 'shared/utils';
import { resolveWsEndpoint, withNetworkOptions } from 'shared/utils/networks';

// A storage entry is callable (returns the value). Map entries also expose `.entries()` for dumps;
// plain `StorageValue` entries (even ones holding a map) do not, hence the optional method.
type StorageQueryFn = ((...keys: unknown[]) => Promise<unknown>) & {
  entries?: (...keys: unknown[]) => Promise<[unknown, unknown][]>;
};
type DynamicQuery = Record<string, Record<string, StorageQueryFn | undefined>>;

// Connect to a chosen `--endpoint`/`--network`, or reuse the process-wide cached client when neither
// is given (it honours CF_NODE_ENDPOINT, falling back to localnet). The result is `await using`-
// friendly: the cached client disposes to a no-op, a one-off remote client disconnects.
async function connect(opts: {
  endpoint?: string;
  network?: string;
}): Promise<DisposableChainflipClient> {
  if (!opts.endpoint && !opts.network) {
    return getChainflipApi();
  }
  const endpoint = resolveWsEndpoint(opts);
  console.error(`Connecting to ${endpoint}`); // stderr keeps stdout clean JSON for piping
  const client = (await DedotClient.new<ChainflipNodeApi>(
    new DedotWsProvider(endpoint),
  )) as DisposableChainflipClient;
  Object.defineProperty(client, Symbol.asyncDispose, {
    configurable: true,
    value: () => client.disconnect(),
  });
  return client;
}

// Try to parse a CLI key as JSON (number/object/array), falling back to the raw string.
function parseKey(s: string): unknown {
  try {
    return JSON.parse(s, bigintReviver);
  } catch {
    return s;
  }
}

// camelCase dedot keys for every pallet that has storage, from the metadata.
function palletsWithStorage(client: ChainflipClient): string[] {
  return client.metadata.latest.pallets
    .filter((p) => p.storage && p.storage.entries.length > 0)
    .map((p) => lowercaseFirstLetter(p.name))
    .sort();
}

// The pallet metadata whose dedot key (camelCased name) is `txPallet`, or undefined.
function findPallet(client: ChainflipClient, txPallet: string) {
  return client.metadata.latest.pallets.find((p) => lowercaseFirstLetter(p.name) === txPallet);
}

// Number of keys an entry takes: 0 for a plain `StorageValue`, 1 for a `StorageMap`, n for an
// n-map. Read from metadata, so we can decide between dumping all entries and an exact lookup
// without a flag.
function storageArity(client: ChainflipClient, txPallet: string, txEntry: string): number {
  const entry = findPallet(client, txPallet)?.storage?.entries.find(
    (e) => lowercaseFirstLetter(e.name) === txEntry,
  );
  if (!entry) {
    return 0;
  }
  return entry.storageType.type === 'Map' ? entry.storageType.value.hashers.length : 0;
}

// camelCase dedot keys for a pallet's storage entries, or null if the pallet isn't found.
function storageEntriesOf(client: ChainflipClient, txPallet: string): string[] | null {
  const pallet = findPallet(client, txPallet);
  if (!pallet || !pallet.storage) {
    return null;
  }
  return pallet.storage.entries.map((e) => lowercaseFirstLetter(e.name)).sort();
}

interface StorageMatch {
  pallet: string;
  entry: string;
  docs: string;
}

// Chain-wide search: case-insensitive substring match of `term` against every `pallet.entry` name
// and its metadata docs, so an item can be found without knowing its pallet. Returns ready-to-use
// `{ pallet, entry }` pairs (plus a docs snippet for disambiguation).
function searchStorage(client: ChainflipClient, term: string): StorageMatch[] {
  const needle = term.toLowerCase();
  const matches: StorageMatch[] = [];
  for (const p of client.metadata.latest.pallets) {
    const pallet = lowercaseFirstLetter(p.name);
    for (const e of p.storage?.entries ?? []) {
      const entry = lowercaseFirstLetter(e.name);
      const docs = e.docs.join(' ').trim();
      if (`${pallet}.${entry} ${docs}`.toLowerCase().includes(needle)) {
        matches.push({ pallet, entry, docs });
      }
    }
  }
  return matches;
}

// Run the query against `client`, printing the result to stdout. Returns nothing.
async function runQuery(
  client: ChainflipClient,
  positional: string[],
  search: string | undefined,
): Promise<void> {
  // Search mode takes precedence over positional pallet/entry.
  if (search !== undefined) {
    console.log(JSON.stringify(searchStorage(client, search), null, 2));
    return;
  }

  const [pallet, entry, ...rawKeys] = positional;

  // No pallet -> list pallets. Pallet but no entry -> list that pallet's entries.
  if (!pallet) {
    console.log(JSON.stringify(palletsWithStorage(client), null, 2));
    return;
  }
  if (!entry) {
    const entries = storageEntriesOf(client, pallet);
    if (!entries) {
      throw new Error(`Unknown pallet '${pallet}'. Run with no args to list pallets.`);
    }
    console.log(JSON.stringify(entries, null, 2));
    return;
  }

  const query = client.query as unknown as DynamicQuery;
  // dedot's query proxy throws on an unknown pallet/entry rather than returning undefined; catch
  // that so we can show the available entries instead of an opaque internal error.
  let storage: StorageQueryFn | undefined;
  try {
    storage = query[pallet]?.[entry];
  } catch {
    storage = undefined;
  }
  if (typeof storage !== 'function') {
    const entries = storageEntriesOf(client, pallet);
    const hint = entries
      ? `Unknown entry '${entry}'. ${pallet} has: ${entries.join(', ')}`
      : `Unknown pallet '${pallet}'. Run with no args to list pallets.`;
    throw new Error(hint);
  }

  const keys = rawKeys.map(parseKey);
  const arity = storageArity(client, pallet, entry);
  if (keys.length > arity) {
    throw new Error(
      `'${pallet}.${entry}' takes ${arity} key(s), got ${keys.length}. ` +
        (arity === 0 ? 'It is a plain value; pass no keys.' : 'Pass fewer keys to prefix-filter.'),
    );
  }

  // A map queried with fewer keys than its arity (including none) dumps all matching entries; a map
  // with a full key set, or a plain value, is read directly. (A plain value may itself hold a map,
  // in which case the direct read returns the whole collection.)
  if (keys.length < arity && typeof storage.entries === 'function') {
    const all = await storage.entries(...keys);
    const out = all.map(([key, value]) => ({ key, value }));
    console.log(JSON.stringify(out, bigintReplacer, 2));
    return;
  }

  const value = await storage(...keys);
  // `undefined` means the entry is unset (no default); print it as null for valid JSON output.
  console.log(JSON.stringify(value ?? null, bigintReplacer, 2));
}

async function main() {
  // `parse-positional-numbers` is disabled so storage keys stay raw strings (JSON-parsed later by
  // parseKey); `--search` aliases `--find`.
  const argv = await withNetworkOptions(
    yargs(hideBin(process.argv)).usage(
      '$0 [pallet] [entry] [...keys] [options] — read a state chain storage value',
    ),
  )
    .option('search', {
      alias: 'find',
      type: 'string',
      describe: 'Substring-search storage entries across all pallets',
    })
    .strictOptions()
    .parserConfiguration({ 'parse-positional-numbers': false })
    .help().argv;

  await using client = await connect({ endpoint: argv.endpoint, network: argv.network });
  await runQuery(client, argv._.map(String), argv.search);
}

// `logExecutionTime: false` keeps stdout pure JSON for piping.
await runWithTimeoutAndExit(main(), 60, false);
