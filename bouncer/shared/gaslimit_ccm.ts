import Web3 from 'web3';
import { Asset } from '@chainflip-io/cli';
import { newCcmMetadata, prepareSwap, testSwap } from './swapping';
import {
  getChainflipApi,
  observeCcmReceived,
  observeEvent,
  observeSwapScheduled,
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

let stopObserving = false;

function gasTestCcmMetadata(sourceAsset: Asset, gasToUse: number) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasToUse]),
    // Very small gas budget since gasPrice in testnet is extremely low (~7wei)
    // 10 ** 9, // => This causes runtime pannic!!
    1,
  );
}

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  gasToUse: number,
  addressType?: BtcAddressType,
) {
  const chainflipApi = await getChainflipApi();

  const messageMetadata = gasTestCcmMetadata(sourceAsset, gasToUse);

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
    () => stopObserving,
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
  swapScheduledObserved = true;
  await swapExecutedHandle;

  const egressGasAmount = Number(swapIdToEgressAmount[swapId].replace(/,/g, ''));

  const ethChainTracking = await observeEvent(
    'ethereumChainTracking:ChainStateUpdated',
    chainflipApi,
  );

  const baseFee = Number(ethChainTracking.data.newChainState.trackedData.baseFee);
  const priorityFee = Number(ethChainTracking.data.newChainState.trackedData.priorityFee);

  // Standard gas estimation
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

  const gasLimitSwapsNotBroadcasted = [
    testGasLimitSwap('DOT', 'FLIP', maximumGasReceived + 20000),
    // testGasLimitSwap('ETH', 'USDC', maximumGasReceived + 20000),
    // testGasLimitSwap('FLIP', 'ETH', maximumGasReceived + 20000),
  ];
  const gasLimitSwapsBroadcasted = [
    // testSwap('DOT', 'FLIP', undefined, gasTestCcmMetadata('DOT', maximumGasReceived), tagSuffix),
    // testSwap('ETH', 'USDC', undefined, gasTestCcmMetadata('ETH', maximumGasReceived), tagSuffix),
    // testSwap('FLIP', 'ETH', undefined, gasTestCcmMetadata('FLIP', maximumGasReceived), tagSuffix),
  ];

  let broadcastAborted = 0;
  await observeEvent(
    'ethereumBroadcaster:BroadcastAborted',
    await getChainflipApi(),
    (_) => ++broadcastAborted === gasLimitSwapsNotBroadcasted.length,
  );

  stopObserving = true;

  // await Promise.all([gasLimitSwapsNotBroadcasted, gasLimitSwapsBroadcasted]);
  await Promise.all(gasLimitSwapsNotBroadcasted);

  console.log('=== GasLimit CCM test completed ===');
}
