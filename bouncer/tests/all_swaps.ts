import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { SwapParams } from '../shared/perform_swap';
import {
  newCcmMetadata,
  newVaultSwapCcmMetadata,
  testSwap,
  testVaultSwap,
} from '../shared/swapping';
import { btcAddressTypes } from '../shared/new_btc_address';
import {
  ccmSupportedChains,
  chainFromAsset,
  VaultSwapParams,
  vaultSwapSupportedChains,
} from '../shared/utils';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testAllSwaps = new ExecutableTest('All-Swaps', main, 3000);

export async function initiateSwap(
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
      sourceAsset,
      destAsset,
      btcAddressTypesArray[Math.floor(Math.random() * btcAddressTypesArray.length)],
      ccmSwapMetadata,
      testAllSwaps.swapContext,
    );
  }
  return functionCall(sourceAsset, destAsset, undefined, ccmSwapMetadata, testAllSwaps.swapContext);
}

async function main() {
  const allSwaps: Promise<SwapParams | VaultSwapParams>[] = [];

  function appendSwap(
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: typeof testSwap | typeof testVaultSwap,
    ccmSwap: boolean = false,
  ) {
    allSwaps.push(initiateSwap(sourceAsset, destAsset, functionCall, ccmSwap));
  }

  Object.values(Assets).forEach((sourceAsset) => {
    Object.values(Assets)
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

  await Promise.all(allSwaps);
}
