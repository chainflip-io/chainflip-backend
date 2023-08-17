import Web3 from 'web3';
import { Asset } from '@chainflip-io/cli';
import { newCcmMetadata, prepareSwap, testSwap } from './swapping';
import {
  getChainflipApi,
  observeCcmReceived,
  observeEvent,
  observeSwapScheduled,
  sleep,
  SwapType,
} from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { BtcAddressType } from './new_btc_address';

// Currently, gasLimit for CCM calls is hardcoded to 400k. Default gas overhead ~115k. Extra margin of
// 215k (CFTester loop step). Should be broadcasted up to ~220k according to contract tests, but the
// parameter values affect that. 210k is a safe bet for a call being broadcasted, 230k won't be.
const maximumGasReceived = 210000;
const tagSuffix = ' CcmGasLimit';

let stopObservingCcmReceived = false;

function gasTestCcmMetadata(sourceAsset: Asset, gasToConsume: number, gasFraction?: number) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasToConsume]),
    // Very small gas budget since gasPrice in testnet is extremely low (~7wei)
    gasFraction, // => 10**9 causes runtime panic in debug mode!!
    // 1,
  );
}

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  gasToConsume: number,
  gasBudgetFraction?: number,
  addressType?: BtcAddressType,
) {
  const chainflipApi = await getChainflipApi();

  const messageMetadata = gasTestCcmMetadata(sourceAsset, gasToConsume, gasBudgetFraction);

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    tagSuffix + ' NoBroadcast',
  );

  const ccmReceivedFailure = observeCcmReceived(
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    () => stopObservingCcmReceived,
  ).then((event) => {
    if (event)
      throw new Error(`${tag} CCM event emitted. Transaction should not have been broadcasted!`);
  });

  const { depositAddress, channelId } = await requestNewSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
  );

  // If sourceAsset === ETH then it won't be swapped so we observe the principal instead
  // If destAsset === ETH then it's still making two swaps, one for the principal and one for the gas
  let egressGasAmount;
  if (sourceAsset !== 'ETH') {
    let swapScheduledObserved = false;

    const swapScheduledHandle = observeSwapScheduled(
      sourceAsset,
      'ETH', // Native destChain asset
      channelId,
      SwapType.CcmGas,
    );

    // SwapExecuted will be emitted at the same time as swapScheduled so we can't wait for swapId
    // to be defined.
    const swapIdToEgressAmount: { [key: string]: string } = {};
    const swapExecutedHandle = observeEvent(
      'swapping:SwapExecuted',
      chainflipApi,
      (event) => {
        swapIdToEgressAmount[event.data.swapId] = event.data.egressAmount;
        return false;
      },
      () => swapScheduledObserved,
    );
    await send(sourceAsset, depositAddress);

    const {
      data: { swapId },
    } = await swapScheduledHandle;

    await sleep(6000); // TO ensure the scheduleExecute has been witnesed
    swapScheduledObserved = true;
    await swapExecutedHandle;
    egressGasAmount = Number(swapIdToEgressAmount[swapId].replace(/,/g, ''));
  } else {
    const swapScheduledHandle = observeSwapScheduled(
      sourceAsset,
      destAsset,
      channelId,
      SwapType.CcmPrincipal,
    );
    await send(sourceAsset, depositAddress);
    await swapScheduledHandle;
    egressGasAmount = messageMetadata.gasBudget;
  }

  const ethChainTracking = await observeEvent(
    'ethereumChainTracking:ChainStateUpdated',
    chainflipApi,
  );

  const baseFee = Number(ethChainTracking.data.newChainState.trackedData.baseFee);
  const priorityFee = Number(ethChainTracking.data.newChainState.trackedData.priorityFee);

  // Standard gas estimation => We might not need to do this as the gasLimit will end up being a bit too high
  const maxFeePerGas = 2 * baseFee + priorityFee;

  // Max Gas Limit budget
  const gasLimitBudget = egressGasAmount / maxFeePerGas;

  // Console log tag egresssGasAmount, fees and gasLimitBudget
  console.log(
    `${tag} egressGasAmount: ${egressGasAmount}, baseFee: ${baseFee}, priorityFee: ${priorityFee}, gasLimitBudget: ${gasLimitBudget}`,
  );

  await ccmReceivedFailure;
}

export async function testGasLimitCcmSwaps() {
  console.log('=== Testing GasLimit CCM swaps ===');

  // As of now, not broadcasted regardless of the gas becuase the gas consumed on the egress is > 400k. However, in localnet
  // this should probably be broadcasted in the future (1% of the amount is enough gas budget since gasPrice is extremely low).
  const gasLimitSwapsAborted = [
    testGasLimitSwap('DOT', 'FLIP', maximumGasReceived + 20000),
    testGasLimitSwap('ETH', 'USDC', maximumGasReceived + 20000),
    testGasLimitSwap('FLIP', 'ETH', maximumGasReceived + 20000),
    testGasLimitSwap('BTC', 'ETH', maximumGasReceived + 20000),
  ];

  // This amount of gas will be swapped into basically zero destination gas. But as of now this will be broadcasted
  // anyway because the gasBudget is not checked (hardcoded to < 400k) and this is within budget. This shouldn't
  // be broadcasted for mainnet.
  const gasLimitSwapsBroadcastedAlmostZeroGas = [
    // testSwap(
    //   'DOT',
    //   'FLIP',
    //   undefined,
    //   gasTestCcmMetadata('DOT', maximumGasReceived, 10 ** 3),
    //   tagSuffix + ' SwappedToZeroGas',
    // ),
    // testSwap(
    //   'ETH',
    //   'USDC',
    //   undefined,
    //   gasTestCcmMetadata('ETH', maximumGasReceived, 10 ** 9),
    //   tagSuffix + ' SwappedToZeroGas',
    // ),
    // testSwap(
    //   'FLIP',
    //   'ETH',
    //   undefined,
    //   gasTestCcmMetadata('FLIP', maximumGasReceived, 10 ** 10),
    //   tagSuffix + ' SwappedToZeroGas',
    // ),
  ];

  // As of now this is broadcasted regardless of the gas budget, but even when the final solution is implemented
  // this should be broadcasted since the gas budget should be enough (same as gasLimitSwapsAborted).
  const gasLimitSwapsBroadcasted = [
    // DEBUG: Swapping these three (at the same time as gasLimitSwapsAborted) don't work => some of these are aborted
    // However, if swapping these three by itself they work fine, so it might be some weird interaction. Even not doing gasTests
    // is causing Broadcast aborted. Something is off. Error on the engine seems to be:
    // Failed to estimate gas\n\nCaused by:\n    (code: -32000, message: execution reverted, data: None)"}
    testSwap(
      'DOT',
      'FLIP',
      undefined,
      gasTestCcmMetadata('DOT', maximumGasReceived),
      // newCcmMetadata('DOT'),
      tagSuffix,
    ),
    testSwap(
      'ETH',
      'USDC',
      undefined,
      gasTestCcmMetadata('ETH', maximumGasReceived),
      // newCcmMetadata('ETH'),
      tagSuffix,
    ),
    testSwap(
      'FLIP',
      'ETH',
      undefined,
      gasTestCcmMetadata('FLIP', maximumGasReceived),
      // newCcmMetadata('FLIP'),
      tagSuffix,
    ),
    // DEBUG: Swapping these three (at the same time as gasLimitSwapsAborted) work
    // testSwap('DOT', 'FLIP'),
    // testSwap('ETH', 'USDC'),
    // testSwap('FLIP', 'ETH'),
  ];
  // await Promise.all(gasLimitSwapsBroadcasted);

  let broadcastAborted = 0;
  let stopObserveAborted = false;
  // TODO: When it works, consider removing the continuous broadcast aborted check to simplify test
  const observeBroadcastAborted = observeEvent(
    'ethereumBroadcaster:BroadcastAborted',
    await getChainflipApi(),
    (_) => {
      ++broadcastAborted;
      console.log('BroadcastAborted ', broadcastAborted);
      if (broadcastAborted === gasLimitSwapsAborted.length) {
        stopObservingCcmReceived = true;
        return false;
      }
      if (broadcastAborted < gasLimitSwapsAborted.length) {
        return false;
      }
      // throw new Error('Broadcast Aborted Unexpected');
      return false;
    },
    // return ++broadcastAborted === gasLimitSwapsAborted.length;
    () => stopObserveAborted,
  );

  await Promise.all([
    ...gasLimitSwapsAborted,
    ...gasLimitSwapsBroadcasted,
    // ...gasLimitSwapsBroadcastedAlmostZeroGas,
  ]);
  stopObserveAborted = true;
  await observeBroadcastAborted;

  console.log('=== GasLimit CCM test completed ===');
}
