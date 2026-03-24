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
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

function shuffle<T>(array: T[]): T[] {
  const result = array;
  for (let i = result.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
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
};

function generateSwapPairs() {
  const sources: Source[] = [];
  const destinations: Destination[] = [];

  // TODO: properly include TRON and BSC assets once they are fully integrated
  // if we include Assethub swaps (HubDot, HubUsdc, HubUsdt) in the all-to-all swaps,
  // the test starts to randomly fail because the assethub node is overloaded.
  const AssetsWithoutAssethub = Object.values(Assets).filter(
    (id) =>
      chainFromAsset(id) !== 'Assethub' &&
      chainFromAsset(id) !== 'Bsc' &&
      chainFromAsset(id) !== 'Tron',
  );

  // populate sources and destination lists
  AssetsWithoutAssethub.forEach((asset) => {
    const chain = chainFromAsset(asset);
    sources.push({ asset, trigger: 'DepositChannel' });
    if (vaultSwapSupportedChains.includes(chain)) {
      sources.push({ asset, trigger: 'VaultSwap' });
    }

    destinations.push({ asset });
  });

  // randomly shuffle sources and destinations
  shuffle(sources);
  shuffle(destinations);

  function randomSource(arg: { exclude?: Asset } = {}): Source {
    const available = AssetsWithoutAssethub.filter((a) => a !== arg.exclude);
    const asset = available[Math.floor(Math.random() * available.length)];
    const chain = chainFromAsset(asset);
    const trigger =
      vaultSwapSupportedChains.includes(chain) && Math.random() > 0.5
        ? 'VaultSwap'
        : 'DepositChannel';
    return { asset, trigger };
  }

  function randomDestination(arg: { exclude?: Asset } = {}): Destination {
    const available = AssetsWithoutAssethub.filter((a) => a !== arg.exclude);
    const asset = available[Math.floor(Math.random() * available.length)];
    return { asset };
  }

  // assign swap pairs
  const pairs: SwapPair[] = [];
  while (sources.length > 0 || destinations.length > 0) {
    const source = sources.pop() || randomSource();
    const destination = destinations.pop() || randomDestination();

    if (source.asset === destination.asset) {
      // push two swaps instead, each with a randomly generated different partner
      const pair1 = { source, destination: randomDestination({ exclude: source.asset }) };
      const pair2 = { source: randomSource({ exclude: destination.asset }), destination };
      pairs.push(pair1, pair2);
    } else {
      pairs.push({ source, destination });
    }
  }

  return pairs;
}

export function testAllSwaps(timeoutPerSwap: number) {
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

  // If we include Assethub swaps (HubDot, HubUsdc, HubUsdt) in the all-to-all swaps,
  // the test starts to randomly fail because the assethub node is overloaded.
  const AssetsForTesting = Object.values(Assets).filter((id) => chainFromAsset(id) !== 'Assethub');

  // we do 2 tests for every input and output
  const pairs = [...generateSwapPairs(), ...generateSwapPairs()];
  for (const { source, destination } of pairs) {
    const testFunction = source.trigger === 'DepositChannel' ? testSwap : testVaultSwap;
    appendSwap(source.asset, destination.asset, testFunction, false);

    // also do ccm version of the same swap if destination supports it
    if (ccmSupportedChains.includes(chainFromAsset(destination.asset))) {
      // bitcoin vault swaps don't support ccm, so we use use ArbEth instead
      const sourceAsset =
        source.asset === 'Btc' && source.trigger === 'VaultSwap' ? 'ArbEth' : source.asset;
      appendSwap(sourceAsset, destination.asset, testFunction, true);
    }
  }

  // Swaps from assethub paired with random chains.
  // NOTE: we don't test swaps *to* assethub here, those tests are run sequentially in
  // `testSwapsToAssethub`.
  const assethubAssets = ['HubDot' as Asset, 'HubUsdc' as Asset, 'HubUsdt' as Asset];
  assethubAssets.sort().forEach((hubAsset) => {
    appendSwap(hubAsset, randomElement(AssetsForTesting), testSwap);
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
