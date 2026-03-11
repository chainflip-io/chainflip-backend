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
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

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

  // Assethub is already disabled, pending removal of assets from Assets enum.
  const AssetsWithoutAssethubAndDot = Object.values(Assets).filter(
    (id) => !id.startsWith('Hub') && id !== 'Dot',
  );

  AssetsWithoutAssethubAndDot.sort().forEach((sourceAsset) => {
    AssetsWithoutAssethubAndDot.sort()
      .filter((destAsset) => sourceAsset !== destAsset)
      .forEach((destAsset) => {
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
      });
  });

  for (const swap of allSwaps) {
    concurrentTest(`AllSwaps > ${swap.name}`, swap.test, timeoutPerSwap, 0, true);
  }
}
