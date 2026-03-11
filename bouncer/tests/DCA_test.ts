import { InternalAsset as Asset } from '@chainflip/cli';
import { randomBytes } from 'crypto';
import assert from 'assert';
import {
  Assets,
  newAssetAddress,
  observeBalanceIncrease,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from 'shared/utils';
import { send } from 'shared/send';
import { getBalance } from 'shared/get_balance';
import { executeVaultSwap, prepareVaultSwapSource, requestNewSwap } from 'shared/perform_swap';
import { DcaParams, FillOrKillParamsX128 } from 'shared/new_swap';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { swappingSwapRequestCompleted } from 'generated/events/swapping/swapRequestCompleted';
import { swappingSwapExecuted } from '../generated/events/swapping/swapExecuted';

async function testDCASwap<A = []>(
  parentCf: ChainflipIO<A>,
  inputAsset: Asset,
  amount: number,
  numberOfChunks: number,
  chunkIntervalBlocks: number,
  swapViaVault = false,
) {
  assert(numberOfChunks > 0, 'Number of chunks must be greater than 0');
  const cf = parentCf.withChildLogger(
    `DCA_test_${inputAsset}_${numberOfChunks}_chunks_at_${chunkIntervalBlocks}_interval`,
  );

  const dcaParams: DcaParams = {
    numberOfChunks,
    chunkIntervalBlocks,
  };
  const fillOrKillParams: FillOrKillParamsX128 = {
    refundAddress: await newAssetAddress(inputAsset, randomBytes(32).toString('hex')),
    minPriceX128: '1',
    retryDurationBlocks: 100,
  };

  const destAsset = inputAsset === Assets.Usdc ? Assets.Flip : Assets.Usdc;

  const destAddress = await newAssetAddress(destAsset, randomBytes(32).toString('hex'));

  const destBalanceBefore = await getBalance(destAsset, destAddress);
  cf.debug(`DCA destination address: ${destAddress}`);

  let swapRequestedEvent;

  if (!swapViaVault) {
    const swapRequest = await requestNewSwap(
      cf,
      inputAsset,
      destAsset,
      destAddress,
      undefined, // messageMetadata
      0, // brokerCommissionBps
      0, // boostFeeBps
      fillOrKillParams,
      dcaParams,
    );

    const depositChannelId = swapRequest.channelId;

    // Deposit the asset
    cf.debug(
      `Sending ${amount} of ${inputAsset} to address: ${swapRequest.depositAddress}  destAddress: ${destAddress}`,
    );
    await send(cf.logger, inputAsset, swapRequest.depositAddress, amount.toString());
    cf.debug(`Sent ${amount} ${inputAsset} to ${swapRequest.depositAddress}`);

    swapRequestedEvent = await observeSwapRequested(
      cf,
      inputAsset,
      destAsset,
      { type: TransactionOrigin.DepositChannel, channelId: depositChannelId },
      SwapRequestType.Regular,
    );
  } else {
    const subcf = cf.with({ account: fullAccountFromUri('//BROKER_1', 'Broker') });
    const source = await prepareVaultSwapSource(subcf, inputAsset, amount.toString());
    const { transactionId } = await executeVaultSwap(
      subcf,
      source,
      inputAsset,
      destAsset,
      destAddress,
      undefined,
      amount.toString(),
      undefined,
      fillOrKillParams,
      dcaParams,
    );

    cf.debug(`Vault swap executed, tx id: ${transactionId}`);

    // Look after Swap Requested of data.origin.Vault.tx_hash
    swapRequestedEvent = await observeSwapRequested(
      cf,
      inputAsset,
      destAsset,
      transactionId,
      SwapRequestType.Regular,
    );
  }

  const swapRequestId = swapRequestedEvent.swapRequestId;
  cf.debug(
    `${inputAsset} swap ${swapViaVault ? 'via vault' : ''}, swapRequestId: ${swapRequestId}`,
  );

  // Find the `SwapExecuted` event of the first chunk
  await cf.stepUntilEvent(
    `Swapping.SwapExecuted`,
    swappingSwapExecuted.refine((event) => event.swapRequestId === swapRequestId),
  );
  cf.debug(`Chunk 1/${numberOfChunks} complete`);

  // Find the remaining chunks
  for (let i = 2; i <= numberOfChunks; i++) {
    // Exactly step chunkIntervalBlocks. This also checks that the chunk interval is correctly observed.
    await cf.stepNBlocks(chunkIntervalBlocks);
    await cf.expectEvent(
      `Swapping.SwapExecuted`,
      swappingSwapExecuted.refine((event) => event.swapRequestId === swapRequestId),
    );
    cf.debug(`Chunk ${i}/${numberOfChunks} complete`);
  }

  // Wait for SwapRequestCompleted, usually it appears at the same block of the last chunk.
  await cf.stepUntilEvent(
    `Swapping.SwapRequestCompleted`,
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === swapRequestId),
  );

  cf.debug(`Chunk interval of ${chunkIntervalBlocks} verified for all ${numberOfChunks} chunks`);

  await observeBalanceIncrease(cf.logger, destAsset, destAddress, destBalanceBefore);
}

export async function testDCASwaps(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await cf.all([
    (subcf) => testDCASwap(subcf, Assets.Eth, 1, 2, 2),
    (subcf) => testDCASwap(subcf, Assets.ArbEth, 1, 4, 1),
    (subcf) => testDCASwap(subcf, Assets.Sol, 10, 2, 3, true),
    (subcf) => testDCASwap(subcf, Assets.SolUsdc, 10, 2, 1, true),
  ]);
}
