import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { observeEvent, observeBadEvent, getChainflipApi } from 'shared/utils/substrate';
import {
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
  sleep,
  newAssetAddress,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

// Test that governance can trigger deposit witnessing for a deposit made with the wrong asset.
// Scenario:
// 1. Open a deposit channel for USDC on Ethereum
// 2. Send USDT (wrong asset) to the deposit address
// 3. Verify no swap is triggered (wrong asset not auto-witnessed)
// 4. Submit governance extrinsic to trigger witnessing with USDT
// 5. Verify the swap completes successfully
export async function testGovernanceDepositWitnessing(testContext: TestContext) {
  const logger = testContext.logger;
  const cf: ChainflipIO<[]> = await newChainflipIO(logger, []);
  // Step 1: Open deposit channel for USDC -> Flip
  const destAddress = await newAssetAddress('Flip', 'GOV_WITNESS_TEST');
  const swapParams = await requestNewSwap(cf, 'Usdc', 'Flip', destAddress);

  logger.info(
    `Deposit channel created: channelId=${swapParams.channelId}, address=${swapParams.depositAddress}`,
  );

  // Step 2: Set up observer to catch unexpected swaps (should NOT trigger during wait period)
  const badSwapObserver = observeBadEvent(logger, 'swapping:SwapRequested', {
    test: (event) => {
      if (typeof event.data.origin === 'object' && 'DepositChannel' in event.data.origin) {
        return Number(event.data.origin.DepositChannel.channelId) === swapParams.channelId;
      }
      return false;
    },
  });

  // Step 3: Send USDT (wrong asset) to USDC deposit address and capture the block number
  logger.info('Sending USDT to USDC deposit channel (should not trigger swap)...');
  const txReceipt = await send(logger, 'Usdt', swapParams.depositAddress);
  const depositBlockNumber = Number(txReceipt.blockNumber);
  logger.info(`USDT deposit transaction included in Ethereum block ${depositBlockNumber}`);

  // Step 4: Wait to confirm no automatic witnessing occurs
  await sleep(30000);

  // Step 5: Stop the bad event observer before triggering governance
  // (since we expect a swap after governance call)
  await badSwapObserver.stop();

  // Step 6: Fetch actual deposit channel details from chain storage
  await using chainflip = await getChainflipApi();

  const depositChannelDetails = (
    await chainflip.query.ethereumIngressEgress.depositChannelLookup(swapParams.depositAddress)
  ).toJSON() as {
    depositChannel: {
      channelId: number;
      address: string;
      asset: string;
      state: string;
    };
  };

  if (!depositChannelDetails) {
    throw new Error(`Deposit channel not found for address ${swapParams.depositAddress}`);
  }

  // Step 7: Set up swap observer before governance call
  const swapRequestedHandle = observeSwapRequested(
    cf,
    'Usdt',
    'Flip',
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  // Step 8: Submit governance extrinsic to trigger witnessing with USDT
  // Use the block number where the USDT deposit happened, and actual deposit channel state from chain
  logger.info(
    `Submitting governance extrinsic to trigger deposit witnessing at block ${depositBlockNumber}...`,
  );
  await submitGovernanceExtrinsic(async (api) => {
    const depositChannel = {
      channelId: depositChannelDetails.depositChannel.channelId,
      address: depositChannelDetails.depositChannel.address,
      asset: 'Usdt', // Override to USDT instead of USDC
      state: depositChannelDetails.depositChannel.state,
    };

    const properties = [depositBlockNumber, { DepositChannels: [depositChannel] }];

    return api.tx.ethereumElections.startNewBlockWitnesserElection(properties);
  }, logger);

  // Step 9: Verify swap was triggered
  const swapEvent = await swapRequestedHandle;
  logger.info(`Swap requested with ID: ${swapEvent.swapRequestId}`);

  // Step 10: Verify swap completes
  await observeEvent(logger, 'swapping:SwapRequestCompleted', {
    test: (event) => BigInt(event.data.swapRequestId) === swapEvent.swapRequestId,
    historicalCheckBlocks: 10,
  }).event;

  logger.info('Test completed successfully! Governance-triggered witnessing worked.');
}
