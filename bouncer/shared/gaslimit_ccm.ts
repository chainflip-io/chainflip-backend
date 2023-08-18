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
    gasFraction,
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
    tagSuffix + ' BroadcastAborted',
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

    // SwapExecuted will be emitted at the same time as swapScheduled so we can't wait for
    // swapId to be known.
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

    // Time buffer ensure the scheduleExecute has been witnessed
    await sleep(6000);
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
    Promise.all([send(sourceAsset, depositAddress), swapScheduledHandle]);
    egressGasAmount = messageMetadata.gasBudget;
  }

  const ethChainTracking = await observeEvent(
    'ethereumChainTracking:ChainStateUpdated',
    chainflipApi,
  );

  const baseFee = Number(ethChainTracking.data.newChainState.trackedData.baseFee);
  const priorityFee = Number(ethChainTracking.data.newChainState.trackedData.priorityFee);

  // Standard gas estimation => In the statechain we might do a less conservative estimation, otherwise
  // a good amount of gas might end up being unused (gasLimit too low).
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
  // TODO: For final solution, add swaps that consume slightly more gas than the gasBudget.
  const gasLimitSwapsAborted = [
    testGasLimitSwap('DOT', 'FLIP', maximumGasReceived + 20000),
    testGasLimitSwap('ETH', 'USDC', maximumGasReceived + 20000),
    testGasLimitSwap('FLIP', 'ETH', maximumGasReceived + 20000),
    testGasLimitSwap('BTC', 'ETH', maximumGasReceived + 20000),
  ];

  // This amount of gas will be swapped into very little destination gas. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap intput amount. So for now we provide the minimum so it gets swapped to just > 0.
  // As of now this will be broadcasted anyway because the gasBudget is not checked (hardcoded to < 400k) and this is within budget.
  // However, this shouldn't be broadcasted for mainnet.
  const gasLimitSwapsBroadcastedAlmostZeroGas = [
    // ~ 410 wei for gasBudget (after CcmGas swap)
    testSwap(
      'DOT',
      'FLIP',
      undefined,
      gasTestCcmMetadata('DOT', maximumGasReceived, 10 ** 6),
      tagSuffix + ' SwappedToMinimalGas',
    ),
    // ~ 500 wei for gasBudget (no CcmGas swap executed)
    testSwap(
      'ETH',
      'USDC',
      undefined,
      gasTestCcmMetadata('ETH', maximumGasReceived, 10 ** 17),
      tagSuffix + ' SwappedToMinimalGas',
    ),
    // ~ 450 wei for gasBudget (after CcmGas swap)
    testSwap(
      'FLIP',
      'ETH',
      undefined,
      gasTestCcmMetadata('FLIP', maximumGasReceived, 2 * 10 ** 6),
      tagSuffix + ' SwappedToMinimalGas',
    ),
  ];

  // As of now this is broadcasted regardless of the gas budget and even when the final solution is implemented
  // this should be broadcasted, since the gas budget should be enough, since by default gasBudget is 1% of the
  // principal and the gasPrice is very low in localnet (~7wei).
  const gasLimitSwapsBroadcasted = [
    testSwap('DOT', 'FLIP', undefined, gasTestCcmMetadata('DOT', maximumGasReceived), tagSuffix),
    testSwap('ETH', 'USDC', undefined, gasTestCcmMetadata('ETH', maximumGasReceived), tagSuffix),
    testSwap('FLIP', 'ETH', undefined, gasTestCcmMetadata('FLIP', maximumGasReceived), tagSuffix),
    testSwap('BTC', 'ETH', undefined, gasTestCcmMetadata('BTC', maximumGasReceived), tagSuffix),
  ];

  let broadcastAborted = 0;
  let stopObserveAborted = false;
  const observeBroadcastAborted = observeEvent(
    'ethereumBroadcaster:BroadcastAborted',
    await getChainflipApi(),
    (_) => {
      ++broadcastAborted;
      if (broadcastAborted === gasLimitSwapsAborted.length) {
        stopObservingCcmReceived = true;
      }
      if (broadcastAborted > gasLimitSwapsAborted.length) {
        throw new Error('Broadcast Aborted Unexpected');
      }
      // Continue observing for unexpepected BroadcastAborted events
      return false;
    },
    () => stopObserveAborted,
  );

  await Promise.all([
    ...gasLimitSwapsAborted,
    ...gasLimitSwapsBroadcasted,
    ...gasLimitSwapsBroadcastedAlmostZeroGas,
  ]);

  stopObserveAborted = true;
  await observeBroadcastAborted;

  console.log('=== GasLimit CCM test completed ===');
}
