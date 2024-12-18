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
  let finished: number = 0;
  let total: number = 0;

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

  function randomElement<Value>(items: Value[]): Value {
    return items[Math.floor(Math.random() * items.length)];
  }

  // if we include Assethub swaps (HubDot, HubUsdc, HubUsdt) in the all-to-all swaps,
  // the test starts to randomly fail because the assethub node is overloaded.

  const AssetsWithoutAssethub = Object.values(Assets).filter((id) => !id.startsWith('Hub'));

  AssetsWithoutAssethub.forEach((sourceAsset) => {
    AssetsWithoutAssethub.filter((destAsset) => sourceAsset !== destAsset).forEach((destAsset) => {
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

  // Swaps from/to assethub paired with random chains
  const assethubAssets = ['HubDot' as Asset, 'HubUsdc' as Asset, 'HubUsdt' as Asset];
  const assets = Object.values(Assets);
  assethubAssets.forEach((hubAsset) => {
    appendSwap(hubAsset, randomElement(assets), testSwap);
    appendSwap(randomElement(assets), hubAsset, testSwap);
  });
  appendSwap('ArbEth', 'HubDot', testVaultSwap);
  appendSwap('ArbEth', 'HubUsdc', testVaultSwap);
  appendSwap('ArbEth', 'HubUsdt', testVaultSwap);

  total = allSwaps.length;

  await Promise.all(
    allSwaps.map((promise) =>
      promise.then(async (result) => {
        finished += 1;
        console.log(`Finished ${finished} of ${total} swaps.`);
        return result;
      }),
    ),
  );
}
