// Shared metadata introspection for the governance `update_pallet_config` calls, used by the
// list_pallet_config_updates / submit_pallet_config_update commands.
//
// Two arities exist across pallets and both are exposed as `client.tx.<pallet>.updatePalletConfig`:
//   - 'array'  -> takes a `BoundedVec<PalletConfigUpdate>` (e.g. swapping, *IngressEgress, pools)
//   - 'single' -> takes a single `PalletConfigUpdate`     (e.g. validator, *Broadcaster, *ThresholdSigner)
// Callers must pass the argument with the matching shape.

import type { ChainflipClient } from 'shared/utils/dedot';
import { lowercaseFirstLetter } from 'shared/utils';

// The runtime call that carries config updates, as it appears (snake_case) in metadata.
export const CONFIG_CALL = 'update_pallet_config';

export type Registry = ChainflipClient['registry'];
export type PortableType = ReturnType<Registry['findType']>;
export type TypeDef = PortableType['typeDef'];
type EnumDef = Extract<TypeDef, { type: 'Enum' }>;
export type EnumMember = EnumDef['value']['members'][number];
export type Field = EnumMember['fields'][number];
export type PalletInfo = ChainflipClient['metadata']['latest']['pallets'][number];

export type ConfigArity = 'single' | 'array';

export interface ConfigCallInfo {
  // dedot tx pallet key, e.g. "swapping" or "ethereumIngressEgress".
  txPallet: string;
  // Whether `updatePalletConfig` expects a single update object or an array of them.
  arity: ConfigArity;
  // The `PalletConfigUpdate` enum type that describes the individual update variants.
  elementType: PortableType;
}

// Resolve the element type id of a Vec-like type id, unwrapping single-field newtype structs such
// as `BoundedVec<T, _>` (metadata models these as a Struct wrapping a `Vec<T>`). Returns null if
// the type isn't a (possibly-wrapped) sequence.
function vecElementTypeId(registry: Registry, typeId: number, depth = 0): number | null {
  if (depth > 4) {
    return null;
  }
  const def = registry.findType(typeId).typeDef;
  if (def.type === 'Sequence') {
    return def.value.typeParam;
  }
  if (def.type === 'Struct' && def.value.fields.length === 1) {
    return vecElementTypeId(registry, def.value.fields[0].typeId, depth + 1);
  }
  return null;
}

// If `pallet` exposes an `update_pallet_config` call, resolve its arity and the config-update enum,
// else return null.
export function resolveConfigCall(registry: Registry, pallet: PalletInfo): ConfigCallInfo | null {
  if (!pallet.calls) {
    return null;
  }
  const callType = registry.findType(pallet.calls.typeId);
  if (callType.typeDef.type !== 'Enum') {
    return null;
  }
  const configCall = callType.typeDef.value.members.find((m) => m.name === CONFIG_CALL);
  if (!configCall || configCall.fields.length === 0) {
    return null;
  }

  const fieldTypeId = configCall.fields[0].typeId;
  const elementTypeId = vecElementTypeId(registry, fieldTypeId);
  // A (wrapped) Vec field -> array arity; a bare enum field -> single arity.
  const arity: ConfigArity = elementTypeId === null ? 'single' : 'array';
  const elementType = registry.findType(elementTypeId ?? fieldTypeId);
  if (elementType.typeDef.type !== 'Enum') {
    return null;
  }

  return { txPallet: lowercaseFirstLetter(pallet.name), arity, elementType };
}

// Resolve the config call for a single dedot tx pallet key, throwing a helpful error if absent.
export function resolveConfigCallByTxPallet(
  client: ChainflipClient,
  txPallet: string,
): ConfigCallInfo {
  const { registry } = client;
  const pallet = client.metadata.latest.pallets.find(
    (p) => lowercaseFirstLetter(p.name) === txPallet,
  );
  const info = pallet && resolveConfigCall(registry, pallet);
  if (info) {
    return info;
  }
  throw new Error(
    `'${txPallet}' has no update_pallet_config call. Run list_pallet_config_updates.ts to see valid pallets.`,
  );
}
