#!/usr/bin/env -S pnpm tsx
// Lists every governance-gated `update_pallet_config` call exposed by the running state chain,
// together with the full shape of each config-update variant, by introspecting the live runtime
// metadata. Nothing is built or submitted — this command is read-only.
//
// An agent uses this to discover which knob to turn and how to shape the payload, then feeds a
// crafted payload to ./commands/submit_pallet_config_update.ts.
//
// Usage (from bouncer/):
//   ./commands/list_pallet_config_updates.ts            # all pallets, JSON to stdout
//   ./commands/list_pallet_config_updates.ts swapping   # filter by pallet (substring, case-insensitive)
//
// Output: JSON keyed by the dedot tx pallet key (e.g. "swapping", "ethereumIngressEgress"). Each
// entry lists the config-update variants in dedot's `{ type, value }` form, plus `arity` — whether
// `updatePalletConfig` takes an array of updates or a single one:
//   {
//     "swapping": {
//       "txPallet": "swapping",
//       "arity": "array",
//       "updateType": "PalletConfigUpdate",
//       "variants": [
//         { "type": "SwapRetryDelay", "value": { "delay": "u32" } },
//         { "type": "MaximumSwapAmount", "value": { "asset": "enum Asset: Eth|Flip|...", "amount": "Option<u128 (pass as string)>" } }
//       ]
//     }
//   }
//
// Field keys are emitted camelCase (matching dedot's codec). Integer types wider than 32 bits are
// annotated "(pass as string)" — pass those values as quoted decimal strings, not JSON numbers.
//
// submit_pallet_config_update.ts always accepts a JSON array and adapts to the arity, so to dry-run
// setting the swap retry delay to 5:
//   echo '[{"type":"SwapRetryDelay","value":{"delay":5}}]' \
//     | ./commands/submit_pallet_config_update.ts swapping - --dry-run

import { getChainflipApi } from 'shared/utils/substrate';
import {
  resolveConfigCall,
  type Field,
  type PortableType,
  type Registry,
} from 'shared/utils/pallet_config';
import { bigintReplacer, runWithTimeoutAndExit } from 'shared/utils';

// Bound recursion when rendering deeply-nested types; beyond this we just print the type name.
const MAX_DEPTH = 5;

// Get the last path segment of a type, so we can look for "PalletConfigUpdate".
const lastPath = (t: PortableType): string =>
  t.path.length ? t.path[t.path.length - 1] : `Type#${t.id}`;

// `Vec<...>`/`Option<...>` etc. need their inner shape as a string; objects render as JSON.
const inline = (v: unknown): string => (typeof v === 'string' ? v : JSON.stringify(v));

// Metadata field names are snake_case, but dedot's codec expects the camelCase keys from its
// generated types. Convert so the emitted shape is directly usable by submit_pallet_config_update.
const toCamel = (s: string): string =>
  s.replace(/_([a-z0-9])/g, (_m, c: string) => c.toUpperCase());

// Integer kinds wider than 32 bits must be passed as quoted decimal strings (decoded to BigInt) —
// a plain JSON number is rejected by dedot's codec. Annotate them so the listing says so.
const BIGINT_KINDS = new Set(['u64', 'u128', 'u256', 'i64', 'i128', 'i256']);

// Turn a list of fields into a JSON-ish shape: named -> object, single unnamed -> newtype,
// many unnamed -> tuple. Used for both struct types and enum variant payloads.
function shapeFields(registry: Registry, fields: readonly Field[], depth: number): unknown {
  // eslint-disable-next-line @typescript-eslint/no-use-before-define
  const one = (f: Field) => describe(registry, f.typeId, depth);
  if (fields.length === 0) {
    return null;
  }
  if (fields.every((f) => f.name !== undefined)) {
    const obj: Record<string, unknown> = {};
    for (const f of fields) {
      obj[toCamel(f.name as string)] = one(f);
    }
    return obj;
  }
  if (fields.length === 1) {
    return one(fields[0]);
  }
  return fields.map(one);
}

// Render a type id into a human-readable JSON-ish description of the value it expects.
function describe(registry: Registry, typeId: number, depth: number): unknown {
  const t = registry.findType(typeId);
  if (depth > MAX_DEPTH) {
    return lastPath(t);
  }
  const def = t.typeDef;
  switch (def.type) {
    case 'Primitive':
      return BIGINT_KINDS.has(def.value.kind)
        ? `${def.value.kind} (pass as string)`
        : def.value.kind;
    case 'Compact':
      return `Compact<${inline(describe(registry, def.value.typeParam, depth + 1))}>`;
    case 'Sequence': {
      const innerSeq = describe(registry, def.value.typeParam, depth + 1);
      return innerSeq === 'u8' ? 'Bytes' : `Vec<${inline(innerSeq)}>`;
    }
    case 'SizedVec': {
      const innerVec = describe(registry, def.value.typeParam, depth + 1);
      return innerVec === 'u8'
        ? `[u8; ${def.value.len}]`
        : `[${inline(innerVec)}; ${def.value.len}]`;
    }
    case 'Tuple':
      return def.value.fields.length
        ? def.value.fields.map((id) => describe(registry, id, depth + 1))
        : null;
    case 'Struct':
      return shapeFields(registry, def.value.fields, depth + 1);
    case 'Enum': {
      const { members } = def.value;
      const isOption =
        lastPath(t) === 'Option' ||
        (members.length === 2 &&
          members.some((m) => m.name === 'None') &&
          members.some((m) => m.name === 'Some'));
      if (isOption) {
        const some = members.find((m) => m.name === 'Some');
        const innerOpt =
          some && some.fields.length ? describe(registry, some.fields[0].typeId, depth + 1) : null;
        return `Option<${inline(innerOpt)}>`;
      }
      if (members.every((m) => m.fields.length === 0)) {
        return `enum ${lastPath(t)}: ${members.map((m) => m.name).join('|')}`;
      }
      return {
        enum: lastPath(t),
        variants: members.map((m) =>
          m.fields.length === 0 ? m.name : { [m.name]: shapeFields(registry, m.fields, depth + 1) },
        ),
      };
    }
    default:
      return lastPath(t);
  }
}

async function main() {
  const filter = process.argv[2]?.toLowerCase();

  await using client = await getChainflipApi();
  const { registry } = client;

  const result: Record<string, unknown> = {};
  for (const pallet of client.metadata.latest.pallets) {
    const info = resolveConfigCall(registry, pallet);
    if (
      info &&
      info.elementType.typeDef.type === 'Enum' &&
      (!filter || info.txPallet.toLowerCase().includes(filter))
    ) {
      result[info.txPallet] = {
        txPallet: info.txPallet,
        arity: info.arity,
        updateType: lastPath(info.elementType),
        variants: info.elementType.typeDef.value.members.map((m) => ({
          type: m.name,
          value: shapeFields(registry, m.fields, 1),
        })),
      };
    }
  }

  console.log(JSON.stringify(result, bigintReplacer, 2));
}

// `logExecutionTime: false` keeps stdout pure JSON for piping.
await runWithTimeoutAndExit(main(), 60, false);
