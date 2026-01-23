import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { getChainflipApi } from 'shared/utils/substrate';
import {
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
  newAssetAddress,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { swappingSwapRequestCompleted } from 'generated/events/swapping/swapRequestCompleted';
import { ethereumChainTrackingChainStateUpdated } from 'generated/events/ethereumChainTracking/chainStateUpdated';
import { ethereumIngressEgressDepositFinalised } from 'generated/events/ethereumIngressEgress/depositFinalised';

// Test that governance can trigger deposit witnessing for a deposit made with the wrong asset.
// Scenario:
// 1. Open a deposit channel for USDC on Ethereum
// 2. Send USDT (wrong asset) to the deposit address
// 3. Verify no swap is triggered (wrong asset not auto-witnessed)
// 4. Submit governance extrinsic to trigger witnessing with USDT
// 5. Verify the swap completes successfully
export async function testGovernanceDepositWitnessing(testContext: TestContext) {
  const cf: ChainflipIO<[]> = await newChainflipIO(testContext.logger, []);
  // Step 1: Open deposit channel for USDC -> Flip
  const destAddress = await newAssetAddress('Flip', 'GOV_WITNESS_TEST');
  const swapParams = await requestNewSwap(cf, 'Usdc', 'Flip', destAddress);

  cf.info(
    `Deposit channel created: channelId=${swapParams.channelId}, address=${swapParams.depositAddress}`,
  );

  // Step 2: Send USDT (wrong asset) to USDC deposit address and capture the block number
  cf.info('Sending USDT to USDC deposit channel (should not trigger swap)...');
  const txReceipt = await send(cf.logger, 'Usdt', swapParams.depositAddress);
  const depositBlockNumber = Number(txReceipt.blockNumber);
  cf.info(`USDT deposit transaction included in Ethereum block ${depositBlockNumber}`);

  // Step 3: Check that no swap is triggered
  const resultEvent = await cf.stepUntilOneEventOf({
    depositFinalized: {
      name: 'EthereumIngressEgress.DepositFinalized',
      schema: ethereumIngressEgressDepositFinalised.refine(
        (event) =>
          event.depositAddress === swapParams.depositAddress &&
          event.channelId === BigInt(swapParams.channelId),
      ),
    },
    ethereumAdvancedEnough: {
      name: 'EthereumChainTracking.ChainStateUpdated',
      schema: ethereumChainTrackingChainStateUpdated.refine(
        (event) => event.newChainState.blockHeight === BigInt(depositBlockNumber + 10),
      ),
    },
  });

  if (resultEvent.key === 'depositFinalized') {
    throw new Error(
      `Unexpected event emitted ${resultEvent.key} in block ${resultEvent.blockHeight}`,
    );
  }
  // Step 4: Fetch actual deposit channel details from chain storage
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

  // Step 5: Submit governance extrinsic to trigger witnessing with USDT
  // Use the block number where the USDT deposit happened, and actual deposit channel state from chain
  cf.info(
    `Submitting governance extrinsic to trigger deposit witnessing at block ${depositBlockNumber}...`,
  );
  await cf.submitGovernance({
    extrinsic: async (api) => {
      const depositChannel = {
        channelId: depositChannelDetails.depositChannel.channelId,
        address: depositChannelDetails.depositChannel.address,
        asset: 'Usdt', // Override to USDT instead of USDC
        state: depositChannelDetails.depositChannel.state,
      };

      const properties = [depositBlockNumber, { DepositChannels: [depositChannel] }];

      return api.tx.ethereumElections.startNewBlockWitnesserElection(properties);
    },
  });

  // Step 6: Wait for swap to be triggered.
  const swapEvent = await observeSwapRequested(
    cf,
    'Usdt',
    'Flip',
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  cf.info(`Swap requested with ID: ${swapEvent.swapRequestId}`);

  // Step 7: Verify swap completes
  await cf.stepUntilEvent(
    'Swapping.SwapRequestCompleted',
    swappingSwapRequestCompleted.refine((event) => event.swapRequestId === swapEvent.swapRequestId),
  );

  cf.info('Test completed successfully! Governance-triggered witnessing worked.');
}
