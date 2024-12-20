import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { SwapParams } from '../shared/perform_swap';
import { newCcmMetadata, testSwap, testVaultSwap } from '../shared/swapping';
import { btcAddressTypes } from '../shared/new_btc_address';
import { ccmSupportedChains, chainFromAsset, VaultSwapParams } from '../shared/utils';

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
          ccmSwap ? newCcmMetadata(destAsset) : undefined,
          testAllSwaps.swapContext,
        ),
      );
    } else {
      allSwaps.push(
        functionCall(
          sourceAsset,
          destAsset,
          undefined,
          ccmSwap ? newCcmMetadata(destAsset) : undefined,
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

  // Not doing BTC due to encoding complexity in vault_swap. Will be fixed once SDK supports it.
  appendSwap('Sol', 'Eth', testVaultSwap);
  appendSwap('Sol', 'Usdc', testVaultSwap, true);
  appendSwap('Sol', 'ArbEth', testVaultSwap);
  appendSwap('Sol', 'ArbEth', testVaultSwap, true);
  appendSwap('Sol', 'Dot', testVaultSwap);
  appendSwap('SolUsdc', 'Eth', testVaultSwap);
  appendSwap('SolUsdc', 'Flip', testVaultSwap, true);

  // Note: Test vault swap with invalid refund params to ensure the engine decoding fallback works.
  // We observed this values in the wild and want to ensure that the engine can handle them gracefully.
  allSwaps.push(
    testVaultSwap(
      "ArbEth",
      "Eth",
      undefined,
      newCcmMetadata("Eth", undefined, undefined, "0xdeadbeef"),
      testAllSwaps.swapContext,
    ),
  );

  allSwaps.push(
    testVaultSwap(
      "Eth",
      "ArbEth",
      undefined,
      newCcmMetadata("ArbEth", undefined, undefined, "0xdeadC0de"),
      testAllSwaps.swapContext,
    ),
  );

  // Invalid refund params: passing no params
  allSwaps.push(
    testVaultSwap(
      "ArbEth",
      "Eth",
      undefined,
      newCcmMetadata("Eth", undefined, undefined, "0x"),
      testAllSwaps.swapContext,
    ),
  );

  await Promise.all(allSwaps);
}
