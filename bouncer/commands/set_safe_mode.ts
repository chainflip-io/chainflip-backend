#!/usr/bin/env -S pnpm tsx
// Sets the runtime safe mode via governance (environment.update_safe_mode).
//
// Safe mode is a single struct of per-pallet flags. `CodeAmber` replaces the WHOLE struct, so
// setting one item is a read-modify-write: read the current safe mode, change the requested flag,
// and submit the whole struct back (all other flags preserved). `CodeRed`/`CodeGreen` are
// whole-runtime shortcuts (everything off / everything on).
//
// Usage (from bouncer/):
//   ./commands/set_safe_mode.ts                                # print the current safe mode
//   ./commands/set_safe_mode.ts swapping swapsEnabled false    # set a boolean flag
//   ./commands/set_safe_mode.ts lendingPools borrowing Red     # set an enum flag  -> { type: 'Red' }
//   ./commands/set_safe_mode.ts witnesser CodeRed              # set a pallet-level enum
//   ./commands/set_safe_mode.ts code-red                       # whole runtime off (CodeRed)
//   ./commands/set_safe_mode.ts code-green                     # whole runtime on  (CodeGreen)
//   add --dry-run to any set to encode + print the call without submitting.
//
// Pallet and flag names are the camelCase keys shown by the no-arg listing. The value is coerced to
// the flag's current type: booleans accept true/false; enum flags/pallets take the variant name.

import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { getChainflipApi } from 'shared/utils/substrate';
import { runWithTimeoutAndExit } from 'shared/utils';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { extrinsicToHumanReadable, type ChainflipClient } from 'shared/utils/dedot';
import type { PalletCfEnvironmentSafeModeUpdate } from 'generated/chaintypes/chainflip-node';

// Decoded safe mode: pallet -> either a struct of flags (bool or nested `{ type }` enum) or, for a
// few pallets (e.g. witnesser), a pallet-level `{ type }` enum.
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

// Apply `<pallet> <flag> <value>` (or `<pallet> <value>` for a pallet-level enum) to `safeMode` in
// place, coercing the value to the target field's current type.
function applySet(safeMode: RuntimeSafeMode, positional: string[]): void {
  const [pallet, second, third] = positional;
  const palletMode = safeMode[pallet];
  if (palletMode === undefined) {
    throw new Error(
      `Unknown safe-mode pallet '${pallet}'. Options: ${Object.keys(safeMode).join(', ')}`,
    );
  }

  if (isEnum(palletMode)) {
    // Pallet-level enum, e.g. `witnesser CodeRed`.
    if (positional.length !== 2) {
      throw new Error(
        `'${pallet}' is a single enum; use: ${pallet} <variant> (current: ${palletMode.type})`,
      );
    }
    palletMode.type = second;
    return;
  }

  // Struct of flags, e.g. `swapping swapsEnabled false`.
  if (positional.length !== 3) {
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
async function buildUpdate(
  client: ChainflipClient,
  positional: string[],
): Promise<PalletCfEnvironmentSafeModeUpdate> {
  const shortcut = CODE_SHORTCUTS[positional[0]];
  if (shortcut) {
    return { type: shortcut };
  }
  const safeMode = (await client.query.environment.runtimeSafeMode()) as unknown as RuntimeSafeMode;
  applySet(safeMode, positional);
  return {
    type: 'CodeAmber',
    value: safeMode,
  } as unknown as PalletCfEnvironmentSafeModeUpdate;
}

async function main() {
  const argv = await yargs(hideBin(process.argv))
    .usage(
      '$0 [pallet] [flag] [value] — set the runtime safe mode via governance (no args lists it)',
    )
    .option('list', {
      type: 'boolean',
      default: false,
      describe: 'Print the current safe mode and exit',
    })
    .option('dry-run', {
      type: 'boolean',
      default: false,
      describe: 'Encode + print the call without submitting',
    })
    .strictOptions()
    .parserConfiguration({ 'parse-positional-numbers': false })
    .help().argv;

  const positional = argv._.map(String);
  const dryRun = argv.dryRun;

  // No positional args (or --list): print the current safe mode and exit.
  if (positional.length === 0 || argv.list) {
    await using client = await getChainflipApi();
    const safeMode = await client.query.environment.runtimeSafeMode();
    console.log(JSON.stringify(safeMode, null, 2));
    return;
  }

  if (dryRun) {
    await using client = await getChainflipApi();
    const update = await buildUpdate(client, positional);
    const ext = client.tx.environment.updateSafeMode(update);
    console.log('DRY RUN — not submitted');
    console.log(`Call: ${extrinsicToHumanReadable(ext)}`);
    console.log(`Encoded call: ${ext.callHex}`);
    return;
  }

  const proposalId = await submitGovernanceExtrinsic(async (client) =>
    client.tx.environment.updateSafeMode(await buildUpdate(client, positional)),
  );
  console.log(`Submitted governance proposal ${proposalId}: environment.updateSafeMode(...)`);
}

await runWithTimeoutAndExit(main(), 120);
