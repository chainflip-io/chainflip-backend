#!/usr/bin/env -S pnpm tsx
// Submits a `<pallet>.update_pallet_config(updates)` call as a snowWhite governance proposal.
// The `updates` payload is a JSON array of dedot `{ type, value }` config-update variants — exactly
// the shape printed by ./commands/list_pallet_config_updates.ts.
//
// Usage (from bouncer/):
//   ./commands/submit_pallet_config_update.ts <txPallet> <updates> [--dry-run]
//
//   <txPallet>  dedot tx pallet key, e.g. "swapping" or "ethereumIngressEgress"
//   <updates>   the JSON array, provided as one of:
//                 - a literal JSON string
//                 - @path/to/file.json   (read from file)
//                 - -                     (read from stdin)
//   --dry-run   build + SCALE-encode the call and print it, but DO NOT submit
//
// Integers: pass values that fit in a JS number as JSON numbers; pass larger integers (u64/u128
// amounts) as quoted decimal strings — they are decoded to BigInt. Non-numeric strings (asset
// names, addresses) are left untouched.
//
// Examples:
//   # Dry-run: set the swap retry delay to 5 blocks
//   echo '[{"type":"SwapRetryDelay","value":{"delay":5}}]' \
//     | ./commands/submit_pallet_config_update.ts swapping - --dry-run
//
//   # Submit: cap the maximum BTC swap amount (large amount as a quoted string)
//   ./commands/submit_pallet_config_update.ts swapping \
//     '[{"type":"MaximumSwapAmount","value":{"asset":"Btc","amount":"100000000"}}]'

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import type { SubmittableExtrinsic } from '@polkadot/api/types';
import {
  getChainflipApi,
  getChainflipPolkadotApi,
  type DisposableApiPromise,
} from 'shared/utils/substrate';
import { submitGovernanceExtrinsic, submitGovernanceExtrinsicPolkadot } from 'shared/cf_governance';
import { extrinsicToHumanReadable, type ChainflipClient } from 'shared/utils/dedot';
import { resolveConfigCallByTxPallet, type ConfigArity } from 'shared/utils/pallet_config';
import { bigintReplacer, bigintReviver, runWithTimeoutAndExit } from 'shared/utils';

// A single `{ type, value }` config-update variant, as printed by list_pallet_config_updates.ts.
type DedotVariant = { type: string; value?: unknown };

// dedot's `client.tx[pallet]` is statically typed; for a runtime-chosen pallet/call we go through a
// minimal structural view. `updatePalletConfig` is the camelCased `update_pallet_config`; depending
// on the pallet it takes either a single update object or an array of them (see ConfigArity).
type UpdateConfigCall = (
  arg: unknown,
) => ReturnType<ChainflipClient['tx']['flip']['updatePalletConfig']>;
type DynamicTx = Record<string, Record<string, UpdateConfigCall | undefined>>;

// Same structural view over the polkadot.js api, used by the fallback encoder.
type PjsUpdateCall = (arg: unknown) => SubmittableExtrinsic<'promise'>;
type PjsDynamicTx = Record<string, Record<string, PjsUpdateCall | undefined>>;

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk as Buffer);
  }
  return Buffer.concat(chunks).toString('utf-8');
}

async function loadUpdatesArg(arg: string): Promise<string> {
  if (arg === '-') {
    return readStdin();
  }
  if (arg.startsWith('@')) {
    const { readFile } = await import('fs/promises');
    return readFile(arg.slice(1), 'utf-8');
  }
  return arg;
}

// Adapt the uniform `updates` array to the call's arity: the array itself for array-arity pallets,
// or the single update for single-arity ones (validator, *Broadcaster, *ThresholdSigner).
function adaptToArity<T>(updates: T[], arity: ConfigArity): T | T[] {
  return arity === 'single' ? updates[0] : updates;
}

// Build `client.tx.<pallet>.updatePalletConfig(...)` via dedot (the primary encoder).
function buildUpdateCall(
  client: ChainflipClient,
  txPallet: string,
  updates: DedotVariant[],
  arity: ConfigArity,
) {
  const tx = client.tx as unknown as DynamicTx;
  const call = tx[txPallet]?.updatePalletConfig;
  if (typeof call !== 'function') {
    throw new Error(`'${txPallet}' has no updatePalletConfig call.`);
  }
  return call(adaptToArity(updates, arity));
}

// Build the same call via polkadot.js (the fallback encoder). dedot's `{ type, value }` variants map
// to polkadot.js's `{ [Variant]: value }` enum form.
function buildPjsCall(
  api: DisposableApiPromise,
  txPallet: string,
  updates: DedotVariant[],
  arity: ConfigArity,
) {
  const pjsUpdates = updates.map((u) => ({ [u.type]: u.value ?? null }));
  const tx = api.tx as unknown as PjsDynamicTx;
  const call = tx[txPallet]?.updatePalletConfig;
  if (typeof call !== 'function') {
    throw new Error(`'${txPallet}' has no updatePalletConfig call.`);
  }
  return call(adaptToArity(pjsUpdates, arity));
}

async function main() {
  const argv = await yargs(hideBin(process.argv))
    .usage('$0 <txPallet> <updates|@file|-> [--dry-run]')
    .option('dry-run', {
      type: 'boolean',
      default: false,
      describe: 'Build + SCALE-encode the call and print it, but do NOT submit',
    })
    .strictOptions()
    .parserConfiguration({ 'parse-positional-numbers': false })
    .help().argv;

  const [txPallet, updatesArg] = argv._.map(String);
  const dryRun = argv.dryRun;

  if (!txPallet || updatesArg === undefined) {
    throw new Error(
      'Usage: submit_pallet_config_update.ts <txPallet> <updates|@file|-> [--dry-run]',
    );
  }

  const raw = await loadUpdatesArg(updatesArg);
  const updates = JSON.parse(raw, bigintReviver) as DedotVariant[];
  if (!Array.isArray(updates)) {
    throw new Error('updates must be a JSON array of { type, value } config-update variants');
  }

  await using client = await getChainflipApi();
  const { arity } = resolveConfigCallByTxPallet(client, txPallet);
  if (arity === 'single' && updates.length !== 1) {
    throw new Error(
      `'${txPallet}'.updatePalletConfig takes a single update; got ${updates.length}. Pass a one-element array.`,
    );
  }

  // Prefer dedot's encoder, but fall back to polkadot.js if dedot can't encode the call. dedot 1.3
  // has an encoding bug for some shapes (e.g. Option<u128> in a tightly-sized config-update union,
  // as in tradingStrategy) that throws a RangeError; polkadot.js encodes those fine.
  let useDedot = true;
  let dedotHex = '';
  let human = '';
  try {
    const ext = buildUpdateCall(client, txPallet, updates, arity);
    dedotHex = ext.callHex; // forces encode + validation
    human = extrinsicToHumanReadable(ext);
  } catch (e) {
    useDedot = false;
    const reason = e instanceof Error ? e.message.split('\n')[0] : String(e);
    console.error(`dedot could not encode this call (${reason}); falling back to polkadot.js.`);
  }

  if (dryRun) {
    console.log('DRY RUN — not submitted');
    console.log(`Updates: ${JSON.stringify(updates, bigintReplacer)}`);
    if (useDedot) {
      console.log('Encoder: dedot');
      console.log(`Call: ${human}`);
      console.log(`Encoded call: ${dedotHex}`);
    } else {
      await using api = await getChainflipPolkadotApi();
      console.log('Encoder: polkadot.js (fallback)');
      console.log(`Encoded call: ${buildPjsCall(api, txPallet, updates, arity).method.toHex()}`);
    }
    return;
  }

  const proposalId = useDedot
    ? await submitGovernanceExtrinsic((c) => buildUpdateCall(c, txPallet, updates, arity))
    : await submitGovernanceExtrinsicPolkadot((api) => buildPjsCall(api, txPallet, updates, arity));

  const via = useDedot ? '' : ' (polkadot.js fallback)';
  if (proposalId < 0) {
    // The extrinsic was submitted and finalized, but the proposal id couldn't be read (indexer down).
    console.log(
      `Submitted ${txPallet}.updatePalletConfig(...)${via} — proposal id unavailable (indexer not reachable).`,
    );
  } else {
    console.log(
      `Submitted governance proposal ${proposalId}: ${txPallet}.updatePalletConfig(...)${via}`,
    );
  }
}

await runWithTimeoutAndExit(main(), 120);
