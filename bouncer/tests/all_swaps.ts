import { InternalAsset as Asset } from '@chainflip/cli';
import { SwapParams } from 'shared/perform_swap';
import { newCcmMetadata, newVaultSwapCcmMetadata, testSwap, testVaultSwap } from 'shared/swapping';
import { btcAddressTypes } from 'shared/new_btc_address';
import {
  Assets,
  ccmSupportedChains,
  chainFromAsset,
  VaultSwapParams,
  vaultSwapSupportedChains,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { manuallyAddTestToList, concurrentTest } from 'shared/utils/vitest';
import { SwapContext } from 'shared/utils/swap_context';

export async function initiateSwap(
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
      testContext.logger,
      sourceAsset,
      destAsset,
      btcAddressTypesArray[Math.floor(Math.random() * btcAddressTypesArray.length)],
      ccmSwapMetadata,
      testContext.swapContext,
    );
  }
  return functionCall(
    testContext.logger,
    sourceAsset,
    destAsset,
    undefined,
    ccmSwapMetadata,
    testContext.swapContext,
  );
}

manuallyAddTestToList('AllSwaps', 'testAllSwaps');

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
        await initiateSwap(context, sourceAsset, destAsset, functionCall, ccmSwap);
      },
    });
  }

  function randomElement<Value>(items: Value[]): Value {
    return items[Math.floor(Math.random() * items.length)];
  }

  // if we include Assethub swaps (HubDot, HubUsdc, HubUsdt) in the all-to-all swaps,
  // the test starts to randomly fail because the assethub node is overloaded.

  const AssetsWithoutAssethubAndDot = Object.values(Assets).filter(
    (id) => !id.startsWith('Hub') && id !== 'Dot',
  );

  AssetsWithoutAssethubAndDot.forEach((sourceAsset) => {
    AssetsWithoutAssethubAndDot.filter((destAsset) => sourceAsset !== destAsset).forEach(
      (destAsset) => {
        // Regular swaps
        appendSwap(sourceAsset, destAsset, testSwap);

        const sourceChain = chainFromAsset(sourceAsset);
        const destChain = chainFromAsset(destAsset);
        if (vaultSwapSupportedChains.includes(sourceChain)) {
          // Vault Swaps
          appendSwap(sourceAsset, destAsset, testVaultSwap);

          // Bitcoin doesn't support CCM Vault swaps due to transaction length limits
          if (ccmSupportedChains.includes(destChain) && sourceChain !== 'Bitcoin') {
            // CCM Vault swaps
            appendSwap(sourceAsset, destAsset, testVaultSwap, true);
          }
        }

        if (ccmSupportedChains.includes(destChain)) {
          // CCM swaps
          appendSwap(sourceAsset, destAsset, testSwap, true);
        }
      },
    );
  });

  // Swaps from assethub paired with random chains.
  // NOTE: we don't test swaps *to* assethub here, those tests are run sequentially in
  // `testSwapsToAssethub`.
  const assethubAssets = ['HubDot' as Asset, 'HubUsdc' as Asset, 'HubUsdt' as Asset];
  assethubAssets.forEach((hubAsset) => {
    appendSwap(hubAsset, randomElement(AssetsWithoutAssethubAndDot), testSwap);
  });

  for (const swap of allSwaps) {
    concurrentTest(`AllSwaps > ${swap.name}`, swap.test, timeoutPerSwap, true);
  }
}

export async function testSwapsToAssethub(testContext: TestContext) {
  // we run three swaps to assethub in sequence. Otherweise there can be nonce issues,
  // which caused bouncer flakiness in the past.
  for (const destinationAsset of ['HubDot', 'HubUsdc', 'HubUsdt'] as Asset[]) {
    const logger = testContext.logger.child({ tag: `ArbEth to ${destinationAsset}` });
    await testSwap(logger, 'ArbEth', destinationAsset, undefined, undefined, new SwapContext());
  }
}
