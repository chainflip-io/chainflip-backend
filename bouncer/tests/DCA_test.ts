import { InternalAsset as Asset, InternalAssets as Assets, InternalAsset } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import { getDefaultProvider, Wallet } from 'ethers';
import assert from 'assert';
import {
  chainFromAsset,
  chainGasAsset,
  getContractAddress,
  getEvmEndpoint,
  newAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
} from '../shared/utils';
import { send } from '../shared/send';
import { observeEvent, observeEvents } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { ExecutableTest } from '../shared/executable_test';
import { requestNewSwap } from '../shared/perform_swap';
import { DcaParams, FillOrKillParamsX128 } from '../shared/new_swap';
import { newCcmMetadata } from '../shared/swapping';
import { executeContractSwap } from '../shared/contract_swap';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testDCASwaps = new ExecutableTest('DCA-Swaps', main, 150);

// Requested number of blocks between each chunk
const CHUNK_INTERVAL = 2;

async function testDCASwap(
  inputAsset: Asset,
  amount: number,
  numberOfChunks: number,
  swapviaContract = false,
) {
  assert(numberOfChunks > 1, 'Number of chunks must be greater than 1');

  const dcaParams: DcaParams = {
    numberOfChunks,
    chunkIntervalBlocks: CHUNK_INTERVAL,
  };
  const fillOrKillParams: FillOrKillParamsX128 = {
    refundAddress: '0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF',
    minPriceX128: '1',
    retryDurationBlocks: 100,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  // TODO: Temporary while we are forced to have CCM when passing VaultSwapAttributes
  const destAddress = !swapviaContract
    ? await newAddress(destAsset, randomBytes(32).toString('hex'))
    : getContractAddress(chainFromAsset(destAsset), 'CFTESTER');

  const destBalanceBefore = await getBalance(inputAsset, destAddress);
  testDCASwaps.debugLog(`DCA destination address: ${destAddress}`);

  let swapRequestedHandle;

  if (!swapviaContract) {
    const swapRequest = await requestNewSwap(
      inputAsset,
      destAsset,
      destAddress,
      'DCA_Test',
      undefined, // messageMetadata
      0, // brokerCommissionBps
      false, // log
      0, // boostFeeBps
      fillOrKillParams,
      dcaParams,
    );

    const depositChannelId = swapRequest.channelId;
    swapRequestedHandle = observeSwapRequested(
      inputAsset,
      destAsset,
      depositChannelId,
      SwapRequestType.Regular,
    );

    // Deposit the asset
    await send(inputAsset, swapRequest.depositAddress, amount.toString());
    testDCASwaps.log(`Sent ${amount} ${inputAsset} to ${swapRequest.depositAddress}`);
  } else {
    const srcChain = chainFromAsset(inputAsset);

    // Probably refactor this into a function
    const mnemonic = Wallet.createRandom().mnemonic?.phrase ?? '';
    if (mnemonic === '') {
      throw new Error('Failed to create random mnemonic');
    }
    const wallet = Wallet.fromPhrase(mnemonic).connect(
      getDefaultProvider(getEvmEndpoint(srcChain)),
    );
    await send(chainGasAsset(srcChain) as InternalAsset, wallet.address);
    await send(inputAsset, wallet.address);

    const contractSwapParams = await executeContractSwap(
      inputAsset,
      destAsset,
      destAddress,
      wallet,
      // Creating CCM metadata because we need a CCM metadata with the current SDK to be able
      // to pass the ccmAdditionalData even if we dont' need it. Then if the gasBudget is
      // very high the swap might fail so we force a lower gasBudget.
      // TODO: Remove the entire CCM metadata.
      newCcmMetadata(inputAsset, destAsset, undefined, 100),
      amount.toString(),
      undefined,
      fillOrKillParams,
      dcaParams,
    );

    // Look after Swap Requested of data.origin.Vault.tx_hash
    swapRequestedHandle = observeSwapRequested(
      inputAsset,
      destAsset,
      contractSwapParams.hash,
      SwapRequestType.Ccm,
    );
  }

  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));
  testDCASwaps.debugLog(
    `${inputAsset} swap ${swapviaContract ? 'via contract' : ''}, swapRequestId: ${swapRequestId}`,
  );

  // Wait for the swap to complete
  await observeEvent(`swapping:SwapRequestCompleted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
  }).event;

  // Find the `SwapExecuted` events for this swap.
  const observeSwapExecutedEvents = await observeEvents(`swapping:SwapExecuted`, {
    test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: numberOfChunks * CHUNK_INTERVAL + 10,
  }).events;

  // Check that there were the correct number of SwapExecuted events, one for each chunk.
  assert.strictEqual(
    observeSwapExecutedEvents.length,
    // TODO: Temporary because of gas swap
    !swapviaContract ? numberOfChunks : numberOfChunks + 1,
    'Unexpected number of SwapExecuted events',
  );

  // TODO: Temporary because of gas swap
  if (!swapviaContract) {
    // Check the chunk interval of all chunks
    for (let i = 1; i < numberOfChunks; i++) {
      const interval = observeSwapExecutedEvents[i].block - observeSwapExecutedEvents[i - 1].block;
      assert.strictEqual(
        interval,
        CHUNK_INTERVAL,
        `Unexpected chunk interval between chunk ${i - 1} & ${i}`,
      );
    }
  }

  testDCASwaps.log(`Chunk interval of ${CHUNK_INTERVAL} verified for all ${numberOfChunks} chunks`);

  await observeBalanceIncrease(destAsset, destAddress, destBalanceBefore);
}

export async function main() {
  await Promise.all([testDCASwap(Assets.Eth, 1, 2), testDCASwap(Assets.ArbEth, 1, 2, true)]);
}
