import * as fs from 'fs/promises';
import * as path from 'path';
// prettier is a dev/tooling-only dependency and this is a codegen helper, not runtime code.
// eslint-disable-next-line import/no-extraneous-dependencies
import prettier from 'prettier';

// Several state-chain pallets are instanced — most per chain (IngressEgress, Broadcaster,
// ChainTracking, Vault, Elections, ...), some per cryptographic scheme (ThresholdSigner). The
// event codegen emits a separate `<prefix><Pallet>/<event>.ts` file for each instance. This
// script groups those files back together into a single prefix-indexed ("generic") event per
// pallet+event, e.g. `generic/thresholdSigner/thresholdSignatureFailed.ts`:
//
//   export const thresholdSignerThresholdSignatureFailedEvent = {
//     Bitcoin: bitcoinThresholdSignerThresholdSignatureFailedEvent,
//     Evm: evmThresholdSignerThresholdSignatureFailedEvent,
//     ...
//   } as const;
//
// Prefixes are not configured anywhere; they are detected from the directory tree itself (see
// resolveInstances). It is pure string/file manipulation over the already-generated flat tree —
// it does not parse metadata or generate any schemas itself.

const cap = (s: string): string => (s ? s[0].toUpperCase() + s.slice(1) : s);
const uncap = (s: string): string => (s ? s[0].toLowerCase() + s.slice(1) : s);

/**
 * Split a directory name into its first camelCase word and the rest, e.g.
 * `bitcoinIngressEgress` -> `bitcoin` + `IngressEgress`. Returns `null` for single-word names
 * (e.g. `swapping`).
 */
const splitFirstWord = (name: string): { prefix: string; suffix: string } | null => {
  const boundary = [...name].findIndex((c) => c >= 'A' && c <= 'Z');
  return boundary > 0 ? { prefix: name.slice(0, boundary), suffix: name.slice(boundary) } : null;
};

const addToSet = (map: Map<string, Set<string>>, key: string, value: string): void => {
  const set = map.get(key) ?? new Set();
  set.add(value);
  map.set(key, set);
};

type ResolvedInstance = {
  /** Instance prefix used as the entry key — a chain (`Bitcoin`) or a crypto scheme (`Evm`). */
  prefix: string;
  /** Pallet name with the prefix stripped, e.g. `ingressEgress`. */
  strippedCamel: string;
};

/**
 * Detect which pallet directories are instanced and split each into `prefix` + pallet name.
 *
 * A pallet suffix counts as instanced when it appears under at least two prefixes, at least one
 * of which is corroborated. A prefix is corroborated when it instances at least two such shared
 * suffixes — true for every chain and crypto scheme, but not for coincidental pairings like
 * `lendingPools`/`liquidityPools` (`liquidity` only precedes one shared suffix, so `Pools` never
 * qualifies). Directories with no qualifying split (e.g. `swapping`) are non-instanced and
 * excluded from the result.
 */
const resolveInstances = (palletDirs: string[]): Map<string, ResolvedInstance> => {
  const splits = new Map(
    palletDirs.flatMap((dir) => {
      const split = splitFirstWord(dir);
      return split ? [[dir, split] as const] : [];
    }),
  );

  const prefixesBySuffix = new Map<string, Set<string>>();
  const suffixesByPrefix = new Map<string, Set<string>>();
  for (const { prefix, suffix } of splits.values()) {
    addToSet(prefixesBySuffix, suffix, prefix);
    addToSet(suffixesByPrefix, prefix, suffix);
  }

  const isShared = (suffix: string): boolean => (prefixesBySuffix.get(suffix)?.size ?? 0) >= 2;
  const isCorroborated = (prefix: string): boolean =>
    [...(suffixesByPrefix.get(prefix) ?? [])].filter(isShared).length >= 2;
  const isInstancedPallet = (suffix: string): boolean =>
    isShared(suffix) && [...(prefixesBySuffix.get(suffix) ?? [])].some(isCorroborated);

  const resolved = new Map<string, ResolvedInstance>();
  for (const [dir, { prefix, suffix }] of splits) {
    if (isInstancedPallet(suffix)) {
      resolved.set(dir, { prefix: cap(prefix), strippedCamel: uncap(suffix) });
    }
  }
  return resolved;
};

type Instance = {
  /** Instance prefix, e.g. `Bitcoin` or `Evm`. */
  prefix: string;
  /** Source pallet directory, e.g. `bitcoinIngressEgress`. */
  palletDir: string;
  /** Event file basename without extension, e.g. `batchBroadcastRequested`. */
  eventBase: string;
};

const renderModule = (constName: string, instances: Instance[]): string => {
  const imports = instances
    .map(
      ({ palletDir, eventBase }) =>
        `import { ${palletDir}${cap(eventBase)}Event } from '../../${palletDir}/${eventBase}';`,
    )
    .join('\n');

  const entries = instances
    .map(({ prefix, palletDir, eventBase }) => `  ${prefix}: ${palletDir}${cap(eventBase)}Event,`)
    .join('\n');

  return `${imports}\n\nexport const ${constName} = {\n${entries}\n} as const;\n`;
};

export default async function generateGenericEvents(eventsDir: string): Promise<void> {
  const genericDir = path.join(eventsDir, 'generic');
  // Clear the output directory
  await fs.rm(genericDir, { recursive: true, force: true });

  const palletDirs = (await fs.readdir(eventsDir, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name);

  // Group key `<strippedPalletCamel>/<eventBase>` -> per-prefix instances
  const groups = new Map<string, Instance[]>();

  for (const [palletDir, { prefix, strippedCamel }] of resolveInstances(palletDirs)) {
    for (const file of await fs.readdir(path.join(eventsDir, palletDir))) {
      if (file.endsWith('.ts')) {
        const eventBase = file.slice(0, -'.ts'.length);
        const key = `${strippedCamel}/${eventBase}`;
        const instances = groups.get(key) ?? [];
        instances.push({ prefix, palletDir, eventBase });
        groups.set(key, instances);
      }
    }
  }

  const prettierConfig = await prettier.resolveConfig(eventsDir);
  let written = 0;

  for (const [key, instances] of groups) {
    // Only emit a generic event for events that exist on more than one instance; a
    // single-instance event has nothing to combine.
    if (instances.length >= 2) {
      instances.sort((a, b) => a.prefix.localeCompare(b.prefix));

      const [strippedCamel, eventBase] = key.split('/');
      const constName = `${strippedCamel}${cap(eventBase)}Event`;
      const source = renderModule(constName, instances);

      const outFile = path.join(genericDir, strippedCamel, `${eventBase}.ts`);
      await fs.mkdir(path.dirname(outFile), { recursive: true });
      await fs.writeFile(
        outFile,
        await prettier.format(source, { ...prettierConfig, parser: 'typescript' }),
        'utf8',
      );
      written += 1;
    }
  }

  console.log(`generated ${written} generic event files in ${genericDir}`);
}
