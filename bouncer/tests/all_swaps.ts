import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
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
import { openPrivateBtcChannel } from '../shared/btc_vault_swap';
import { TestContext } from '../shared/utils/test_context';

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

export async function testAllSwaps(textContext: TestContext) {
  const allSwaps: Promise<SwapParams | VaultSwapParams>[] = [];

  // Open a private BTC channel to be used for btc vault swaps
  await openPrivateBtcChannel(textContext.logger, '//BROKER_1');

  textContext.logger.info(`🧪 Private broker channel opened`);

  function appendSwap(
    sourceAsset: Asset,
    destAsset: Asset,
    functionCall: typeof testSwap | typeof testVaultSwap,
    ccmSwap: boolean = false,
  ) {
    allSwaps.push(initiateSwap(textContext, sourceAsset, destAsset, functionCall, ccmSwap));
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

  // Swaps from/to assethub paired with random chains
  const assethubAssets = ['HubDot' as Asset, 'HubUsdc' as Asset, 'HubUsdt' as Asset];
  assethubAssets.forEach((hubAsset) => {
    appendSwap(hubAsset, randomElement(AssetsWithoutAssethub), testSwap);
    appendSwap(randomElement(AssetsWithoutAssethub), hubAsset, testSwap);
  });
  appendSwap('ArbEth', 'HubDot', testVaultSwap);
  appendSwap('ArbEth', 'HubUsdc', testVaultSwap);
  appendSwap('ArbEth', 'HubUsdt', testVaultSwap);

  textContext.logger.info(`🧪 All swaps appended`);
  await Promise.all(allSwaps);
}
