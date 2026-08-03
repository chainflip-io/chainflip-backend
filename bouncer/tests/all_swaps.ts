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
import { TestContext } from 'shared/utils/test_context';
import { manuallyAddTestToList, concurrentTest } from 'shared/utils/vitest';
import { SwapContext } from 'shared/utils/swap_context';
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

  function randomElement<Value>(items: Value[]): Value {
    return items[Math.floor(Math.random() * items.length)];
  }

  // All assets that should be tested. Filter out assets here if needed.
  // If we include Assethub swaps (HubDot, HubUsdc, HubUsdt) in the all-to-all swaps,
  // the test starts to randomly fail because the assethub node is overloaded.
  const allTestedAssets = Object.values(Assets).filter((id) => chainFromAsset(id) !== 'Assethub');

  const pairs = generateSwapPairs(
    allTestedAssets,
    thoroughlyTestedAssets,
    seededRng(GENERATE_SWAPS_SEED),
  );
  for (const { source, destination, ccm } of pairs) {
    const testFunction = source.trigger === 'DepositChannel' ? testSwap : testVaultSwap;
    appendSwap(source.asset, destination.asset, testFunction, ccm);
  }

  // Swaps from assethub paired with random chains.
  // NOTE: we don't test swaps *to* assethub here, those tests are run sequentially in
  // `testSwapsToAssethub`.
  const assethubAssets = ['HubDot' as Asset, 'HubUsdc' as Asset, 'HubUsdt' as Asset];
  assethubAssets.sort().forEach((hubAsset) => {
    appendSwap(hubAsset, randomElement(allTestedAssets), testSwap);
  });

  for (const swap of allSwaps) {
    concurrentTest(`AllSwaps > ${swap.name}`, swap.test, timeoutPerSwap, 0, true);
  }
}

export async function testSwapsToAssethub(testContext: TestContext) {
  // we run three swaps to assethub in sequence. Otherwise, there can be nonce issues,
  // which caused bouncer flakiness in the past.
  for (const destinationAsset of ['HubDot', 'HubUsdc', 'HubUsdt'] as Asset[]) {
    const logger = testContext.logger.child({ tag: `ArbEth to ${destinationAsset}` });
    const cf = await newChainflipIO(logger, [] as []);
    await testSwap(cf, 'ArbEth', destinationAsset, undefined, undefined, new SwapContext());
  }
}
