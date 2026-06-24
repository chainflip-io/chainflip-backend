import { chainflipChains, legacyChainflipChains } from '@chainflip/utils/chainflip';
import * as fs from 'fs/promises';
import * as path from 'path';
// prettier is a dev/tooling-only dependency and this is a codegen helper, not runtime code.
// eslint-disable-next-line import/no-extraneous-dependencies
import prettier from 'prettier';

// Several state-chain pallets are instanced per chain (IngressEgress, Broadcaster,
// ChainTracking, ThresholdSigner, Vault, Elections, ...). The event codegen emits a
// separate `<chain><Pallet>/<event>.ts` file for each instance. This script groups
// those per-chain files back together into a single indexed event per pallet+event,
// e.g. `aggregated/ingressEgress/batchBroadcastRequested.ts`:
//
//   export const ingressEgressBatchBroadcastRequestedEvent = {
//     Bitcoin: bitcoinIngressEgressBatchBroadcastRequestedEvent,
//     Ethereum: ethereumIngressEgressBatchBroadcastRequestedEvent,
//     ...
//   } as const;
//
// It is pure string/file manipulation over the already-generated flat tree — it does
// not parse metadata or generate any schemas itself.

const cap = (s: string): string => (s ? s[0].toUpperCase() + s.slice(1) : s);
const uncap = (s: string): string => (s ? s[0].toLowerCase() + s.slice(1) : s);

const chains = [...chainflipChains, ...legacyChainflipChains];

// Some pallets are instanced per *cryptographic scheme* rather than per chain, so one generated
// directory backs several chains via a non-chain prefix (e.g. `evmThresholdSigner`).
const cryptoInstanceChains: Record<string, string[]> = {
  evm: ['Ethereum', 'Arbitrum'],
  polkadot: ['Assethub'],
};

const instancePrefixes = [...chains, ...Object.keys(cryptoInstanceChains)];
const instancePrefix = new RegExp(`^(${instancePrefixes.join('|')})(.+)$`, 'i');

type Instance = {
  /** Canonical ChainflipChain name, e.g. `Bitcoin`. */
  chain: string;
  /** Source pallet directory, e.g. `bitcoinIngressEgress`. */
  palletDir: string;
  /** Event file basename without extension, e.g. `batchBroadcastRequested`. */
  eventBase: string;
  /** True if the directory is named after `chain` itself; an exact match wins over a crypto expansion. */
  exact: boolean;
};

// Resolve a pallet directory to the stripped pallet name and the chains it serves (each flagged
// `exact` if the directory is named after that chain). Returns `null` for non-instanced pallets
// (e.g. `genericElections`, `swapping`).
const resolveInstance = (
  palletDir: string,
): { strippedCamel: string; instanceChains: { chain: string; exact: boolean }[] } | null => {
  const match = palletDir.match(instancePrefix);
  if (!match) {
    return null;
  }
  const token = match[1].toLowerCase();
  const exactChain = chains.find((c) => c.toLowerCase() === token);
  const instanceChains = [
    ...(exactChain ? [{ chain: exactChain, exact: true }] : []),
    ...(cryptoInstanceChains[token] ?? []).map((chain) => ({ chain, exact: false })),
  ];
  return instanceChains.length > 0 ? { strippedCamel: uncap(match[2]), instanceChains } : null;
};

const renderModule = (constName: string, instances: Instance[]): string => {
  // A crypto-shared instance (e.g. `evmThresholdSigner`) backs multiple chains via the same
  // binding, so dedupe imports while keeping one entry per chain below.
  const imports = [
    ...new Set(
      instances.map(
        ({ palletDir, eventBase }) =>
          `import { ${palletDir}${cap(eventBase)}Event } from '../../${palletDir}/${eventBase}';`,
      ),
    ),
  ].join('\n');

  const entries = instances
    .map(({ chain, palletDir, eventBase }) => `  ${chain}: ${palletDir}${cap(eventBase)}Event,`)
    .join('\n');

  return `${imports}\n\nexport const ${constName} = {\n${entries}\n} as const;\n`;
};

export default async function generateAggregatedEvents(eventsDir: string): Promise<void> {
  const aggregatedDir = path.join(eventsDir, 'aggregated');
  // Clear the output directory
  await fs.rm(aggregatedDir, { recursive: true, force: true });

  // Group key `<strippedPalletCamel>/<eventBase>` -> per-chain instances
  const groups = new Map<string, Instance[]>();

  for (const palletEntry of await fs.readdir(eventsDir, { withFileTypes: true })) {
    const palletDir = palletEntry.name;
    const resolved = palletEntry.isDirectory() ? resolveInstance(palletDir) : null;

    if (resolved) {
      const { strippedCamel, instanceChains } = resolved;

      for (const file of await fs.readdir(path.join(eventsDir, palletDir))) {
        if (file.endsWith('.ts')) {
          const eventBase = file.slice(0, -'.ts'.length);
          const key = `${strippedCamel}/${eventBase}`;
          const instances = groups.get(key) ?? [];
          for (const { chain, exact } of instanceChains) {
            instances.push({ chain, palletDir, eventBase, exact });
          }
          groups.set(key, instances);
        }
      }
    }
  }

  const prettierConfig = await prettier.resolveConfig(eventsDir);
  let written = 0;
  const skipped: string[] = [];

  for (const [key, rawInstances] of groups) {
    // Pick one instance per chain, preferring a chain's own directory over a crypto expansion
    // (e.g. `assethubBroadcaster` wins over the `polkadot` crypto expansion).
    const byChain = new Map<string, Instance>();
    for (const inst of rawInstances) {
      const existing = byChain.get(inst.chain);
      if (!existing || (!existing.exact && inst.exact)) {
        byChain.set(inst.chain, inst);
      }
    }
    const instances = [...byChain.values()];

    // Only aggregate events that exist on more than one chain; a single-chain event has nothing
    // to aggregate. Record the rest rather than dropping them silently, so a genuinely-missing
    // instance (e.g. a chain whose pallet directory failed to resolve) is visible in the output.
    if (instances.length < 2) {
      skipped.push(`${key} (only ${instances[0].chain})`);
    } else {
      instances.sort((a, b) => a.chain.localeCompare(b.chain));

      const [strippedCamel, eventBase] = key.split('/');
      const constName = `${strippedCamel}${cap(eventBase)}Event`;
      const source = renderModule(constName, instances);

      const outFile = path.join(aggregatedDir, strippedCamel, `${eventBase}.ts`);
      await fs.mkdir(path.dirname(outFile), { recursive: true });
      await fs.writeFile(
        outFile,
        await prettier.format(source, { ...prettierConfig, parser: 'typescript' }),
        'utf8',
      );
      written += 1;
    }
  }

  console.log(`generated ${written} aggregated event files in ${aggregatedDir}`);
  if (skipped.length > 0) {
    console.log(
      `skipped ${skipped.length} single-chain event(s) (not aggregated):\n  ${skipped.sort().join('\n  ')}`,
    );
  }
}
