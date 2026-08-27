#!/usr/bin/env -S pnpm tsx
// Governance helpers as one command with two subcommands. Both submit snowWhite proposals (which
// auto-execute on localnet), so try `--dry-run` first.
//
//   ./commands/governance.ts config <txPallet> <updates|@file|->   submit update_pallet_config call(s)
//   ./commands/governance.ts safe-mode [pallet] [flag] [value]     set the runtime safe mode (no args prints it)
//
// Run `./commands/governance.ts <subcommand> --help` for options.
//
// --- config: finding the update shape (the generated types are the source of truth; these run only
//     on same-commit localnets, so they match) ---
//   1. In generated/chaintypes/chainflip-node/tx.d.ts, find <txPallet>.updatePalletConfig; its
//      parameter names the config-update union, e.g. swapping -> Array<PalletCfSwappingPalletConfigUpdate>.
//   2. Look that union up in types.d.ts: a list of `{ type: 'Variant'; value: {...} }`. Copy `type`
//      verbatim and fill in `value`. Pass the shape the parameter in step 1 shows: a JSON array
//      `[{...}]` for most pallets, or a single `{...}` object for single-arity pallets (validator,
//      *Broadcaster, *ThresholdSigner).
//   Value gotchas: camelCase keys; a `bigint` field (u64/u128) must be a quoted decimal string while
//   `number` (u32-) is a plain number; omitting an `Option` (`foo?:`) means None (clears it); units
//   are domain-specific (Permill, 6-dp USD, blocks) — check state-chain/pallets/<pallet>/src/lib.rs.
//
// --- safe-mode: `CodeAmber` replaces the WHOLE struct, so setting one item is a read-modify-write.
//     The value is coerced to the target flag's current type (bool, or an enum variant name). ---
//   ./commands/governance.ts safe-mode                             # print the current safe mode
//   ./commands/governance.ts safe-mode swapping swapsEnabled false # boolean flag
//   ./commands/governance.ts safe-mode lendingPools borrowing Red  # enum flag       -> { type: 'Red' }
//   ./commands/governance.ts safe-mode witnesser CodeRed           # pallet-level enum
//   ./commands/governance.ts safe-mode code-red | code-green       # whole runtime off / on
//
// Examples:
//   ./commands/governance.ts config swapping '[{"type":"SwapRetryDelay","value":{"delay":5}}]' --dry-run
//   ./commands/governance.ts safe-mode swapping swapsEnabled false

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import type { SubmittableExtrinsic } from '@polkadot/api/types';
import {
  getChainflipApi,
  getChainflipPolkadotApi,
  type DisposableApiPromise,
} from 'shared/utils/substrate';
import { submitGovernanceExtrinsic, submitGovernanceExtrinsicPolkadot } from 'shared/cf_governance';
import { extrinsicToHumanReadable, findPallet, type ChainflipClient } from 'shared/utils/dedot';
import type { PalletCfEnvironmentSafeModeUpdate } from 'generated/chaintypes/chainflip-node';
import { bigintReviver, lowercaseFirstLetter, runWithTimeoutAndExit } from 'shared/utils';

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

function reportProposal(proposalId: number, desc: string, via = ''): void {
  if (proposalId < 0) {
    // Submitted + finalized, but the proposal id couldn't be read (indexer down).
    console.log(`Submitted ${desc}${via} — proposal id unavailable (indexer not reachable).`);
  } else {
    console.log(`Submitted governance proposal ${proposalId}: ${desc}${via}`);
  }
}

// ---------------------------------------------------------------------------
// `config` subcommand — update_pallet_config
// ---------------------------------------------------------------------------

// A `{ type, value }` config-update variant (see the header for how to find its shape).
type DedotVariant = { type: string; value?: unknown };
// The argument to `updatePalletConfig`: an array of variants for most pallets, or a single variant
// for single-arity pallets (validator, *Broadcaster, *ThresholdSigner). Pass whichever shape the
// pallet's `updatePalletConfig` parameter in tx.d.ts shows.
type ConfigPayload = DedotVariant | DedotVariant[];

// dedot's `client.tx[pallet]` is statically typed; for a runtime-chosen pallet/call we go through a
// minimal structural view.
type UpdateConfigCall = (
  arg: unknown,
) => ReturnType<ChainflipClient['tx']['flip']['updatePalletConfig']>;
type DynamicTx = Record<string, Record<string, UpdateConfigCall | undefined>>;
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
  // yargs-parser collapses a bare `-` positional to an empty string, so both mean stdin.
  if (arg === '-' || arg === '') {
    return readStdin();
  }
  if (arg.startsWith('@')) {
    const { readFile } = await import('fs/promises');
    return readFile(arg.slice(1), 'utf-8');
  }
  return arg;
}

// dedot's tx proxy throws on *property access* for an unknown pallet, so a typo would surface from
// inside the encode step and be misreported as a codec failure. Validate against metadata first.
const CONFIG_CALL = 'update_pallet_config';

function palletCallNames(client: ChainflipClient, typeId: number | undefined): string[] {
  if (typeId === undefined) {
    return [];
  }
  const def = client.registry.findType(typeId).typeDef;
  return def.type === 'Enum' ? def.value.members.map((m) => m.name) : [];
}

// camelCase tx keys for every pallet exposing `update_pallet_config`.
function configPallets(client: ChainflipClient): string[] {
  return client.metadata.latest.pallets
    .filter((p) => palletCallNames(client, p.calls?.typeId).includes(CONFIG_CALL))
    .map((p) => lowercaseFirstLetter(p.name))
    .sort();
}

function assertHasConfigCall(client: ChainflipClient, txPallet: string): void {
  const pallet = findPallet(client, txPallet);
  if (pallet && palletCallNames(client, pallet.calls?.typeId).includes(CONFIG_CALL)) {
    return;
  }
  throw new Error(
    `'${txPallet}' has no updatePalletConfig call. Pallets that do: ${configPallets(client).join(', ')}`,
  );
}

// The payload (single variant or array) is passed straight through — the caller shapes it to match
// the pallet's `updatePalletConfig` parameter.
function buildUpdateCall(client: ChainflipClient, txPallet: string, payload: ConfigPayload) {
  const tx = client.tx as unknown as DynamicTx;
  const call = tx[txPallet]?.updatePalletConfig;
  if (typeof call !== 'function') {
    throw new Error(`'${txPallet}' has no updatePalletConfig call.`);
  }
  return call(payload);
}

// polkadot.js fallback encoder: dedot's `{ type, value }` variants map to `{ [Variant]: value }`.
function buildPjsCall(api: DisposableApiPromise, txPallet: string, payload: ConfigPayload) {
  const toPjs = (v: DedotVariant) => ({ [v.type]: v.value ?? null });
  const arg = Array.isArray(payload) ? payload.map(toPjs) : toPjs(payload);
  const tx = api.tx as unknown as PjsDynamicTx;
  const call = tx[txPallet]?.updatePalletConfig;
  if (typeof call !== 'function') {
    throw new Error(`'${txPallet}' has no updatePalletConfig call.`);
  }
  return call(arg);
}

async function runConfig(txPallet: string, updatesArg: string, dryRun: boolean): Promise<void> {
  const raw = await loadUpdatesArg(updatesArg);
  const payload = JSON.parse(raw, bigintReviver) as ConfigPayload;
  const variants = Array.isArray(payload) ? payload : [payload];
  if (variants.length === 0 || variants.some((v) => typeof v?.type !== 'string')) {
    throw new Error(
      'updates must be a { type, value } variant object, or a non-empty array of them',
    );
  }
  const desc = `${txPallet}.updatePalletConfig(${variants.map((v) => v.type).join(', ')})`;

  await using client = await getChainflipApi();
  assertHasConfigCall(client, txPallet);

  // Prefer dedot's encoder; fall back to polkadot.js if it can't encode (dedot 1.3 has a codec bug
  // for some shapes, e.g. Option<u128> in a tightly-sized union as in tradingStrategy).
  let useDedot = true;
  let dedotHex = '';
  let human = '';
  try {
    const ext = buildUpdateCall(client, txPallet, payload);
    dedotHex = ext.callHex; // forces encode + validation
    human = extrinsicToHumanReadable(ext);
  } catch (e) {
    useDedot = false;
    const reason = e instanceof Error ? e.message.split('\n')[0] : String(e);
    console.error(`dedot could not encode this call (${reason}); falling back to polkadot.js.`);
  }

  if (dryRun) {
    console.log(`DRY RUN — not submitted: ${desc}`);
    if (useDedot) {
      console.log('Encoder: dedot');
      console.log(`Call: ${human}`);
      console.log(`Encoded call: ${dedotHex}`);
    } else {
      await using api = await getChainflipPolkadotApi();
      console.log('Encoder: polkadot.js (fallback)');
      console.log(`Encoded call: ${buildPjsCall(api, txPallet, payload).method.toHex()}`);
    }
    return;
  }

  const proposalId = useDedot
    ? await submitGovernanceExtrinsic((c) => buildUpdateCall(c, txPallet, payload))
    : await submitGovernanceExtrinsicPolkadot((api) => buildPjsCall(api, txPallet, payload));
  reportProposal(proposalId, desc, useDedot ? '' : ' (polkadot.js fallback)');
}

// ---------------------------------------------------------------------------
// `safe-mode` subcommand — update_safe_mode
// ---------------------------------------------------------------------------

// Decoded safe mode: pallet -> struct of flags (bool or nested `{ type }` enum) or a pallet-level
// `{ type }` enum (e.g. witnesser).
type FlagValue = boolean | { type: string };
type PalletSafeMode = Record<string, FlagValue> | { type: string };
type RuntimeSafeMode = Record<string, PalletSafeMode>;

const CODE_SHORTCUTS: Record<string, 'CodeRed' | 'CodeGreen'> = {
  'code-red': 'CodeRed',
  'code-green': 'CodeGreen',
};

const isEnum = (v: unknown): v is { type: string } =>
  typeof v === 'object' && v !== null && 'type' in v;

function parseBool(s: string): boolean {
  const v = s.toLowerCase();
  if (['true', '1', 'on', 'yes', 'enabled'].includes(v)) {
    return true;
  }
  if (['false', '0', 'off', 'no', 'disabled'].includes(v)) {
    return false;
  }
  throw new Error(`Expected a boolean (true/false) for a boolean flag, got '${s}'`);
}

// Apply `<pallet> <flag> <value>` (or `<pallet> <value>` for a pallet-level enum) in place, coercing
// the value to the target field's current type.
function applySet(safeMode: RuntimeSafeMode, params: string[]): void {
  const [pallet, second, third] = params;
  const palletMode = safeMode[pallet];
  if (palletMode === undefined) {
    throw new Error(
      `Unknown safe-mode pallet '${pallet}'. Options: ${Object.keys(safeMode).join(', ')}`,
    );
  }

  if (isEnum(palletMode)) {
    // Pallet-level enum, e.g. `witnesser CodeRed`.
    if (params.length !== 2) {
      throw new Error(
        `'${pallet}' is a single enum; use: ${pallet} <variant> (current: ${palletMode.type})`,
      );
    }
    palletMode.type = second;
    return;
  }

  // Struct of flags, e.g. `swapping swapsEnabled false`.
  if (params.length !== 3) {
    throw new Error(
      `'${pallet}' needs a flag and value: ${pallet} <flag> <value>. Flags: ${Object.keys(palletMode).join(', ')}`,
    );
  }
  const current = palletMode[second];
  if (current === undefined) {
    throw new Error(
      `Unknown flag '${second}' on '${pallet}'. Flags: ${Object.keys(palletMode).join(', ')}`,
    );
  }
  palletMode[second] = typeof current === 'boolean' ? parseBool(third) : { type: third };
}

// Build the SafeModeUpdate on `client` (reads + edits the current safe mode for the CodeAmber case).
async function buildSafeModeUpdate(
  client: ChainflipClient,
  params: string[],
): Promise<PalletCfEnvironmentSafeModeUpdate> {
  const shortcut = CODE_SHORTCUTS[params[0]];
  if (shortcut) {
    return { type: shortcut };
  }
  const safeMode = (await client.query.environment.runtimeSafeMode()) as unknown as RuntimeSafeMode;
  applySet(safeMode, params);
  return { type: 'CodeAmber', value: safeMode } as unknown as PalletCfEnvironmentSafeModeUpdate;
}

async function runSafeMode(params: string[], dryRun: boolean, list: boolean): Promise<void> {
  await using client = await getChainflipApi();

  // No params (or --list): print the current safe mode and exit.
  if (params.length === 0 || list) {
    const safeMode = await client.query.environment.runtimeSafeMode();
    console.log(JSON.stringify(safeMode, null, 2));
    return;
  }

  if (dryRun) {
    const ext = client.tx.environment.updateSafeMode(await buildSafeModeUpdate(client, params));
    console.log('DRY RUN — not submitted: environment.updateSafeMode(...)');
    console.log(`Call: ${extrinsicToHumanReadable(ext)}`);
    console.log(`Encoded call: ${ext.callHex}`);
    return;
  }

  const proposalId = await submitGovernanceExtrinsic(async (c) =>
    c.tx.environment.updateSafeMode(await buildSafeModeUpdate(c, params)),
  );
  reportProposal(proposalId, 'environment.updateSafeMode(...)');
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

interface Args {
  _: (string | number)[];
  txPallet?: string;
  updates?: string;
  params?: string[];
  dryRun?: boolean;
  list?: boolean;
}

async function main() {
  const argv = (await yargs(hideBin(process.argv))
    .scriptName('governance.ts')
    .command('config <txPallet> <updates>', 'Submit pallet config update(s) via governance', (y) =>
      y
        .positional('txPallet', {
          type: 'string',
          describe: 'dedot tx pallet key, e.g. "swapping" or "ethereumIngressEgress"',
        })
        .positional('updates', {
          type: 'string',
          describe: 'JSON array of { type, value }, @path/to/file.json, or - for stdin',
        })
        .option('dry-run', {
          type: 'boolean',
          default: false,
          describe: 'Build + SCALE-encode and print the call, but do NOT submit',
        }),
    )
    .command(
      'safe-mode [params..]',
      'Set the runtime safe mode via governance (no args prints the current safe mode)',
      (y) =>
        y
          .positional('params', {
            type: 'string',
            array: true,
            describe: '<pallet> <flag> <value> | <pallet> <variant> | code-red | code-green',
          })
          .option('list', {
            type: 'boolean',
            default: false,
            describe: 'Print the current safe mode and exit',
          })
          .option('dry-run', {
            type: 'boolean',
            default: false,
            describe: 'Encode + print the call, but do NOT submit',
          }),
    )
    .demandCommand(1, 'Specify a subcommand: config or safe-mode')
    .strict()
    .parserConfiguration({ 'parse-positional-numbers': false })
    .help()
    .parseAsync()) as unknown as Args;

  const dryRun = Boolean(argv.dryRun);
  switch (argv._[0]) {
    case 'config':
      await runConfig(String(argv.txPallet), String(argv.updates), dryRun);
      break;
    case 'safe-mode':
      await runSafeMode((argv.params ?? []).map(String), dryRun, Boolean(argv.list));
      break;
    default:
      throw new Error(`Unknown subcommand '${String(argv._[0])}'. Use 'config' or 'safe-mode'.`);
  }
}

await runWithTimeoutAndExit(main(), 120);
