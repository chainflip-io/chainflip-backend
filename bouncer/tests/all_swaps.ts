import { SwapParams } from 'shared/perform_swap';
import { newCcmMetadata, newVaultSwapCcmMetadata, testSwap, testVaultSwap } from 'shared/swapping';
import { btcAddressTypes } from 'shared/new_btc_address';
import {
  Assets,
  ccmSupportedChains,
  chainFromAsset,
  VaultSwapParams,
  vaultSwapSupportedChains,
  Asset,
} from 'shared/utils';
import { describe, expect, test } from 'vitest';
import { TestContext } from 'shared/utils/test_context';
import { manuallyAddTestToList, concurrentTest } from 'shared/utils/vitest';
import { seededRng } from 'shared/utils/seeded_rng';
import { globalLogger } from 'shared/utils/logger';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

// Seed for the sampled swap set. Picked at random each run so coverage rotates over time; the value
// is logged when the test is built so a failing run is reproducible. Pin it via the ALL_SWAPS_SEED
// env var — which `run_test.ts <swap_number> <seed>` sets for you.
const seedOverride = process.env.ALL_SWAPS_SEED;
const GENERATE_SWAPS_SEED =
  seedOverride !== undefined && seedOverride !== ''
    ? Number(seedOverride)
    : Math.floor(Math.random() * 1_000_000);

// Returns a shuffled copy
function shuffle<T>(array: T[], rng: () => number): T[] {
  const result = [...array];
  for (let i = result.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [result[i], result[j]] = [result[j], result[i]];
  }
  return result;
}

export async function initiateSwap(
  cf: ChainflipIO<[]>,
  testContext: TestContext,
  sourceAsset: Asset,
  destAsset: Asset,
  functionCall: typeof testSwap | typeof testVaultSwap,
  ccmSwap: boolean = false,
): Promise<SwapParams | VaultSwapParams> {
  let ccmSwapMetadata;
  if (ccmSwap) {
    ccmSwapMetadata =
      functionCall === testSwap
        ? await newCcmMetadata(destAsset)
        : await newVaultSwapCcmMetadata(sourceAsset, destAsset);
  }

  if (destAsset === 'Btc') {
    const btcAddressTypesArray = Object.values(btcAddressTypes);
    return functionCall(
      cf,
      sourceAsset,
      destAsset,
      btcAddressTypesArray[Math.floor(Math.random() * btcAddressTypesArray.length)],
      ccmSwapMetadata,
      testContext.swapContext,
    );
  }
  return functionCall(
    cf,
    sourceAsset,
    destAsset,
    undefined,
    ccmSwapMetadata,
    testContext.swapContext,
  );
}

manuallyAddTestToList('AllSwaps', 'testAllSwaps');

type Source = {
  asset: Asset;
  trigger: 'DepositChannel' | 'VaultSwap';
};

type Destination = {
  asset: Asset;
};

type SwapPair = {
  source: Source;
  destination: Destination;
  ccm: boolean;
};

/**
 * Builds the set of swap pairs to test, sampling rather than enumerating the full asset matrix.
 *
 * `allTestedAssets` assets are guaranteed to appear at least once as a source and at least
 * once as a destination.
 *
 * `testWithAllPossiblePartners` assets are additionally paired with every
 * other asset in both directions, and via a vault swap wherever the source chain supports it.
 *
 * Each returned pair carries a `ccm` flag: pairs whose destination chain supports CCM are emitted
 * both as a plain swap and as a CCM swap.
 */
function generateSwapPairs(
  allTestedAssets: Asset[],
  testWithAllPossiblePartners: Asset[],
  rng: () => number,
) {
  let sources: Source[] = [];
  let destinations: Destination[] = [];

  for (const asset of testWithAllPossiblePartners) {
    if (!allTestedAssets.includes(asset)) {
      throw new Error(
        `Asset ${asset} that's supposed to be tested against all assets is not included in the list of all assets.`,
      );
    }
  }

  // populate sources and destination lists
  allTestedAssets.forEach((asset) => {
    const chain = chainFromAsset(asset);
    sources.push({ asset, trigger: 'DepositChannel' });
    destinations.push({ asset });
    if (vaultSwapSupportedChains.includes(chain)) {
      sources.push({ asset, trigger: 'VaultSwap' });
    }
  });

  function randomSource(arg: { exclude?: Asset } = {}): Source {
    const available = allTestedAssets.filter((a) => a !== arg.exclude);
    const asset = available[Math.floor(rng() * available.length)];
    const chain = chainFromAsset(asset);
    const trigger =
      vaultSwapSupportedChains.includes(chain) && rng() > 0.5 ? 'VaultSwap' : 'DepositChannel';
    return { asset, trigger };
  }

  function randomDestination(arg: { exclude?: Asset } = {}): Destination {
    const available = allTestedAssets.filter((a) => a !== arg.exclude);
    const asset = available[Math.floor(rng() * available.length)];
    return { asset };
  }

  const pairs: SwapPair[] = [];

  // Append all pairs for assets that should be tested against all
  function pushSwap(trigger: 'DepositChannel' | 'VaultSwap', source: Asset, destination: Asset) {
    pairs.push({
      source: { asset: source, trigger },
      destination: { asset: destination },
      ccm: false,
    });
  }
  for (const asset1 of testWithAllPossiblePartners) {
    for (const asset2 of allTestedAssets) {
      if (asset1 !== asset2) {
        pushSwap('DepositChannel', asset1, asset2);
        pushSwap('DepositChannel', asset2, asset1);
        if (vaultSwapSupportedChains.includes(chainFromAsset(asset1))) {
          pushSwap('VaultSwap', asset1, asset2);
        }
        if (vaultSwapSupportedChains.includes(chainFromAsset(asset2))) {
          pushSwap('VaultSwap', asset2, asset1);
        }
      }
    }
  }

  // Add non-ccm swaps. Each asset will be a source and destination.
  sources = shuffle(sources, rng);
  destinations = shuffle(destinations, rng);
  while (sources.length > 0 || destinations.length > 0) {
    const source = sources.pop() || randomSource();
    const destination = destinations.pop() || randomDestination();

    if (source.asset === destination.asset) {
      // push two swaps instead, each with a randomly generated different partner
      const pair1 = {
        source,
        destination: randomDestination({ exclude: source.asset }),
        ccm: false,
      };
      const pair2 = {
        source: randomSource({ exclude: destination.asset }),
        destination,
        ccm: false,
      };
      pairs.push(pair1, pair2);
    } else {
      pairs.push({ source, destination, ccm: false });
    }
  }

  // Add CCM swaps
  const expandedPairs: SwapPair[] = [];
  for (const { source, destination } of pairs) {
    expandedPairs.push({ source, destination, ccm: false });
    if (ccmSupportedChains.includes(chainFromAsset(destination.asset))) {
      // bitcoin vault swaps don't support ccm, so we use ArbEth instead
      const ccmSource: Source =
        source.asset === 'Btc' && source.trigger === 'VaultSwap'
          ? { asset: 'ArbEth', trigger: source.trigger }
          : source;
      expandedPairs.push({ source: ccmSource, destination, ccm: true });
    }
  }

  // Deduplicate
  const seenPairs = new Set<string>();
  const uniquePairs = expandedPairs.filter((pair) => {
    const key = `${pair.source.asset}-${pair.source.trigger}-${pair.destination.asset}-${pair.ccm}`;
    if (seenPairs.has(key)) return false;
    seenPairs.add(key);
    return true;
  });

  // Check asset coverage
  const sourcedAssets = new Set(uniquePairs.map((pair) => pair.source.asset));
  const destinedAssets = new Set(uniquePairs.map((pair) => pair.destination.asset));
  const missingSources = allTestedAssets.filter((asset) => !sourcedAssets.has(asset));
  const missingDestinations = allTestedAssets.filter((asset) => !destinedAssets.has(asset));
  if (missingSources.length > 0 || missingDestinations.length > 0) {
    throw new Error(
      `generateSwapPairs coverage gap — never a source: [${missingSources.join(', ')}]; ` +
        `never a destination: [${missingDestinations.join(', ')}]`,
    );
  }

  // Shuffle
  return shuffle(uniquePairs, rng);
}

// `thoroughlyTestedAssets` is to help test 100% coverage of new assets when a new chain is added.
// Add the new assets in fast_bouncer.ts during development, and then remove them after release to help keep the number of swaps down.
export function testAllSwaps(timeoutPerSwap: number, thoroughlyTestedAssets: Asset[] = []) {
  globalLogger.info(
    `AllSwaps generated with seed ${GENERATE_SWAPS_SEED}. ` +
      `To reproduce a specific swap: ./commands/run_test.ts <swap_number> ${GENERATE_SWAPS_SEED}`,
  );

  const allSwaps: { name: string; test: (context: TestContext) => Promise<void> }[] = [];
  let allSwapsCount = 0;

  function appendSwap(
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: typeof testSwap | typeof testVaultSwap,
    ccmSwap: boolean = false,
  ) {
    allSwapsCount++;
    const swapType = functionCall === testSwap ? 'Swap' : 'VaultSwap';
    allSwaps.push({
      name: `Swap ${allSwapsCount}: ${sourceAsset} to ${destAsset} (${ccmSwap ? 'CCM ' : ''}${swapType})`,
      test: async (context) => {
        const cf = await newChainflipIO(context.logger, [] as []);
        await initiateSwap(cf, context, sourceAsset, destAsset, functionCall, ccmSwap);
      },
    });
  }

  // All assets that should be tested. Filter out assets here if needed.
  // TODO: properly include TRON and BSC assets once they are fully integrated
  const allTestedAssets = Object.values(Assets).filter((id) => {
    const chain: string = chainFromAsset(id);
    return chain !== 'Bsc';
  });

  const pairs = generateSwapPairs(
    allTestedAssets,
    thoroughlyTestedAssets,
    seededRng(GENERATE_SWAPS_SEED),
  );
  for (const { source, destination, ccm } of pairs) {
    const testFunction = source.trigger === 'DepositChannel' ? testSwap : testVaultSwap;
    appendSwap(source.asset, destination.asset, testFunction, ccm);
  }

  for (const swap of allSwaps) {
    concurrentTest(`AllSwaps > ${swap.name}`, swap.test, timeoutPerSwap, 0, true);
  }
}

// Unit tests to make sure that the coverage of the "AllSwaps" test is correct.
describe('checkAllSwapsCoverage', () => {
  const assets: Asset[] = ['Eth', 'Btc', 'Trx'];
  const SEED = 1;
  const swaps = generateSwapPairs(assets, [], seededRng(SEED));

  const expectedSwaps = [
    {
      source: { asset: 'Eth', trigger: 'DepositChannel' },
      destination: { asset: 'Btc' },
      ccm: false,
    },
    {
      source: { asset: 'Btc', trigger: 'DepositChannel' },
      destination: { asset: 'Trx' },
      ccm: true,
    },
    {
      source: { asset: 'Trx', trigger: 'DepositChannel' },
      destination: { asset: 'Eth' },
      ccm: true,
    },
    { source: { asset: 'Eth', trigger: 'VaultSwap' }, destination: { asset: 'Btc' }, ccm: false },
    {
      source: { asset: 'Btc', trigger: 'DepositChannel' },
      destination: { asset: 'Trx' },
      ccm: false,
    },
    { source: { asset: 'Btc', trigger: 'VaultSwap' }, destination: { asset: 'Trx' }, ccm: false },
    {
      source: { asset: 'Eth', trigger: 'DepositChannel' },
      destination: { asset: 'Trx' },
      ccm: true,
    },
    { source: { asset: 'ArbEth', trigger: 'VaultSwap' }, destination: { asset: 'Trx' }, ccm: true },
    {
      source: { asset: 'Eth', trigger: 'DepositChannel' },
      destination: { asset: 'Trx' },
      ccm: false,
    },
    {
      source: { asset: 'Trx', trigger: 'DepositChannel' },
      destination: { asset: 'Eth' },
      ccm: false,
    },
  ];

  test('produces the expected deterministic swap list for the fixed seed', () => {
    expect(swaps).toEqual(expectedSwaps);
  });

  test('exercises every asset as both a source and a destination', () => {
    const sources = new Set(swaps.map((s) => s.source.asset));
    const destinations = new Set(swaps.map((s) => s.destination.asset));
    for (const asset of assets) {
      expect(sources.has(asset)).toBe(true);
      expect(destinations.has(asset)).toBe(true);
    }
  });

  test('only emits CCM swaps to CCM-supported destination chains', () => {
    for (const swap of swaps.filter((s) => s.ccm)) {
      expect(ccmSupportedChains).toContain(chainFromAsset(swap.destination.asset));
    }
  });

  test('emits a CCM counterpart for every swap to a CCM-supported destination', () => {
    for (const swap of swaps.filter(
      (s) => !s.ccm && ccmSupportedChains.includes(chainFromAsset(s.destination.asset)),
    )) {
      // Bitcoin vault swaps don't support CCM, so their CCM counterpart uses ArbEth as the source.
      const expectedSource =
        swap.source.asset === 'Btc' && swap.source.trigger === 'VaultSwap'
          ? { asset: 'ArbEth', trigger: 'VaultSwap' }
          : swap.source;
      expect(swaps).toContainEqual({
        source: expectedSource,
        destination: swap.destination,
        ccm: true,
      });
    }
  });

  test('only emits vault swaps from vault-supported source chains', () => {
    for (const swap of swaps.filter((s) => s.source.trigger === 'VaultSwap')) {
      expect(vaultSwapSupportedChains).toContain(chainFromAsset(swap.source.asset));
    }
  });

  test('never emits a Bitcoin vault-swap CCM (those are remapped to ArbEth)', () => {
    expect(
      swaps.some((s) => s.ccm && s.source.asset === 'Btc' && s.source.trigger === 'VaultSwap'),
    ).toBe(false);
  });

  test('contains no duplicate swaps', () => {
    const keys = swaps.map(
      (s) => `${s.source.asset}-${s.source.trigger}-${s.destination.asset}-${s.ccm}`,
    );
    expect(new Set(keys).size).toBe(keys.length);
  });
});
