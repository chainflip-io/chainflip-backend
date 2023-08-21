import Web3 from 'web3';
import { Asset, Assets } from '@chainflip-io/cli';
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

// This test uses the CFTester contract as the receiver for a CCM call. The contract will consume approximately
// the gas amount specified in the CCM message with an error margin. On top of that, the gas overhead of the
// CCM call itself is ~115k with some variability depending on the parameters. Overrall, the transaction should
// be broadcasted up to ~220k according to contract tests with some margin (~20k). 210k is a safe bet for a call
// being broadcasted, 230k shouldn't be.
const maximumGasReceived = 210000;
const gasBudgetMargin = 20000;
const tagSuffix = ' CcmGasLimit';

let stopObservingCcmReceived = false;

function gasTestCcmMetadata(sourceAsset: Asset, gasToConsume: number, gasBudgetFraction?: number) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasToConsume]),
    gasBudgetFraction,
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
    tagSuffix,
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

  // If sourceAsset is ETH then deposited gasAmount won't be swapped, so we need to observe the principal swap
  // instead. In any other scenario, including when destAsset is ETH, both principal and gas are being swapped.
  let egressGasAmount;
  if (sourceAsset !== Assets.ETH) {
    const swapScheduledHandle = observeSwapScheduled(
      sourceAsset,
      Assets.ETH, // Native destChain asset
      channelId,
      SwapType.CcmGas,
    );

    // SwapExecuted is emitted at the same time as swapScheduled so we can't wait for swapId to be known.
    const swapIdToEgressAmount: { [key: string]: string } = {};
    let swapScheduledObserved = false;
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

    while (!(swapId in swapIdToEgressAmount)) {
      await sleep(3000);
    }
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

  const ethTrackedData = (
    await observeEvent('ethereumChainTracking:ChainStateUpdated', chainflipApi)
  ).data.newChainState.trackedData;

  const baseFee = Number(ethTrackedData.baseFee);
  const priorityFee = Number(ethTrackedData.priorityFee);

  // Standard gas estimation => In the statechain we might do a less conservative estimation, otherwise
  // a good amount of gas might end up being unused (gasLimit too low).
  const maxFeePerGas = 2 * baseFee + priorityFee;

  // Max Gas Limit budget
  const gasLimitBudget = egressGasAmount / maxFeePerGas;

  // TODO: Add a check for the gasLimitBudget once mainnet logic is implemented. For now just logging it.
  console.log(
    `${tag} egressGasAmount: ${egressGasAmount}, baseFee: ${baseFee}, priorityFee: ${priorityFee}, gasLimitBudget: ${gasLimitBudget}`,
  );

  await ccmReceivedFailure;
}

// NOTE: In localnet the gasPrice is is extremely low (~7wei) so the gasBudget needed is very small.
export async function testGasLimitCcmSwaps() {
  console.log('=== Testing GasLimit CCM swaps ===');

  // As of now, these won't ne broadcasted regardless of the gasBudget provided because the gas consumed on the egress is > 400k.
  // However, with the final solution, in localnet this might be broadcasted (1% of the amount might be enough gas budget since
  // gasPrice is extremely low).
  // TODO: For final solution, make swaps that consume more gas than the gasBudget.
  const gasLimitSwapsAborted = [
    testGasLimitSwap('DOT', 'FLIP', maximumGasReceived + gasBudgetMargin),
    testGasLimitSwap('ETH', 'USDC', maximumGasReceived + gasBudgetMargin),
    testGasLimitSwap('FLIP', 'ETH', maximumGasReceived + gasBudgetMargin),
    testGasLimitSwap('BTC', 'ETH', maximumGasReceived + gasBudgetMargin),
  ];

  // This amount of gas will be swapped into very little destination gas. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap intput amount. So for now we provide the minimum so it gets swapped to just > 0.
  // As of now this will be broadcasted anyway because the gasBudget is not checked (hardcoded to < 400k) and this is within budget.
  // However, this shouldn't be broadcasted for mainnet.
  const gasLimitSwapsInsufBudget = [
    // ~ 410 wei for gasBudget (after CcmGas swap)
    testSwap(
      'DOT',
      'FLIP',
      undefined,
      gasTestCcmMetadata('DOT', maximumGasReceived, 10 ** 6),
      tagSuffix + ' InsufficientGasBudget',
    ),
    // ~ 500 wei for gasBudget (no CcmGas swap executed)
    testSwap(
      'ETH',
      'USDC',
      undefined,
      gasTestCcmMetadata('ETH', maximumGasReceived, 10 ** 17),
      tagSuffix + ' InsufficientGasBudget',
    ),
    // ~ 450 wei for gasBudget (after CcmGas swap)
    testSwap(
      'FLIP',
      'ETH',
      undefined,
      gasTestCcmMetadata('FLIP', maximumGasReceived, 2 * 10 ** 6),
      tagSuffix + ' InsufficientGasBudget',
    ),
  ];

  // As of now this is broadcasted regardless of the gas budget and even when the final solution is implemented
  // this should be broadcasted, since the gas budget should be enough, since by default gasBudget is 1% of the
  // principal and the gasPrice is very low in localnet (~7wei).
  const gasLimitSwapsBroadcasted = [
    testSwap(
      'DOT',
      'FLIP',
      undefined,
      gasTestCcmMetadata('DOT', maximumGasReceived),
      tagSuffix + ' SufficientGasBudget',
    ),
    testSwap(
      'ETH',
      'USDC',
      undefined,
      gasTestCcmMetadata('ETH', maximumGasReceived),
      tagSuffix + ' SufficientGasBudget',
    ),
    testSwap(
      'FLIP',
      'ETH',
      undefined,
      gasTestCcmMetadata('FLIP', maximumGasReceived),
      tagSuffix + ' SufficientGasBudget',
    ),
    testSwap(
      'BTC',
      'ETH',
      undefined,
      gasTestCcmMetadata('BTC', maximumGasReceived),
      tagSuffix + ' SufficientGasBudget',
    ),
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
      // Continue observing for unexpected BroadcastAborted events
      return false;
    },
    () => stopObserveAborted,
  );

  await Promise.all([
    ...gasLimitSwapsAborted,
    ...gasLimitSwapsBroadcasted,
    ...gasLimitSwapsInsufBudget,
  ]);

  stopObserveAborted = true;
  await observeBroadcastAborted;

  console.log('=== GasLimit CCM test completed ===');
}
