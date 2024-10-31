import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { VaultSwapParams } from '../shared/vault_swap';
import { ExecutableTest } from '../shared/executable_test';
import { SwapParams } from '../shared/perform_swap';
import { newCcmMetadata, testSwap, testVaultSwap } from '../shared/swapping';
import { btcAddressTypes } from '../shared/new_btc_address';
import { ccmSupportedChains, chainFromAsset } from '../shared/utils';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testAllSwaps = new ExecutableTest('All-Swaps', main, 3000);

async function main() {
  const allSwaps: Promise<SwapParams | VaultSwapParams>[] = [];

  function appendSwap(
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: typeof testSwap | typeof testVaultSwap,
    ccmSwap: boolean = false,
  ) {
    if (destAsset === 'Btc') {
      const btcAddressTypesArray = Object.values(btcAddressTypes);
      allSwaps.push(
        functionCall(
          sourceAsset,
          destAsset,
          btcAddressTypesArray[Math.floor(Math.random() * btcAddressTypesArray.length)],
          ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
          testAllSwaps.swapContext,
        ),
      );
    } else {
      allSwaps.push(
        functionCall(
          sourceAsset,
          destAsset,
          undefined,
          ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
          testAllSwaps.swapContext,
        ),
      );
    }
  }

  Object.values(Assets).forEach((sourceAsset) => {
    Object.values(Assets)
      .filter((destAsset) => sourceAsset !== destAsset)
      .forEach((destAsset) => {
        // Regular swaps
        appendSwap(sourceAsset, destAsset, testSwap);

        const sourceChain = chainFromAsset(sourceAsset);
        const destChain = chainFromAsset(destAsset);
        if (sourceChain === 'Ethereum' || sourceChain === 'Arbitrum') {
          // Vault Swaps
          appendSwap(sourceAsset, destAsset, testVaultSwap);

          if (ccmSupportedChains.includes(destChain)) {
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

  await Promise.all(allSwaps);
}
