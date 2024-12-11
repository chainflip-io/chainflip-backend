import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { SwapParams } from '../shared/perform_swap';
import { newCcmMetadata, testSwap, testVaultSwap } from '../shared/swapping';
import { btcAddressTypes } from '../shared/new_btc_address';
import { ccmSupportedChains, chainFromAsset, sleep, VaultSwapParams } from '../shared/utils';

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
        sleep(getRandomInt(0, 10000)).then(async (_) =>
          functionCall(
            sourceAsset,
            destAsset,
            btcAddressTypesArray[Math.floor(Math.random() * btcAddressTypesArray.length)],
            ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
            testAllSwaps.swapContext,
          ),
        ),
      );
    } else {
      allSwaps.push(
        sleep(getRandomInt(0, 10000)).then(async (_) =>
          functionCall(
            sourceAsset,
            destAsset,
            undefined,
            ccmSwap ? newCcmMetadata(sourceAsset, destAsset) : undefined,
            testAllSwaps.swapContext,
          ),
        ),
      );
    }
  }

  function getRandomInt(min: number, max: number) {
    const minCeiled = Math.ceil(min);
    const maxFloored = Math.floor(max);
    return Math.floor(Math.random() * (maxFloored - minCeiled) + minCeiled); // The maximum is exclusive and the minimum is inclusive
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
