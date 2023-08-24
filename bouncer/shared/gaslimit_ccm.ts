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
import { sendEth } from './send_eth';

// This test uses the CFTester contract as the receiver for a CCM call. The contract will consume approximately
// the gas amount specified in the CCM message with an error margin. On top of that, the gas overhead of the
// CCM call itself is ~115k with some variability depending on the parameters. Overrall, the transaction should
// be broadcasted up to ~220k according to contract tests with some margin (~20k). 210k is a safe bet for a call
// being broadcasted, 230k shouldn't be.
const GAS_OVERHEAD = 170000;
const DEFAULT_GAS_CONSUMPTION = 200000;
const tagSuffix = ' CcmGasLimit';

let stopObservingCcmReceived = false;

function gasTestCcmMetadata(sourceAsset: Asset, gasToConsume?: number, gasBudgetFraction?: number) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
  const gasConsumption = gasToConsume ?? DEFAULT_GAS_CONSUMPTION;
  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasConsumption]),
    gasBudgetFraction,
  );
}

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  gasToConsume?: number,
  gasBudgetFraction?: number,
  addressType?: BtcAddressType,
) {
  const chainflipApi = await getChainflipApi();

  const gasConsumption = gasToConsume ?? DEFAULT_GAS_CONSUMPTION;

  const messageMetadata = gasTestCcmMetadata(sourceAsset, gasConsumption, gasBudgetFraction);

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    tagSuffix,
  );

  const ccmReceived = observeCcmReceived(
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    () => stopObservingCcmReceived,
  );

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
  let swapScheduledHandle;
  if (sourceAsset === Assets.ETH) {
    swapScheduledHandle = observeSwapScheduled(
      sourceAsset,
      destAsset,
      channelId,
      SwapType.CcmPrincipal,
    );
  } else {
    swapScheduledHandle = observeSwapScheduled(sourceAsset, Assets.ETH, channelId, SwapType.CcmGas);
  }

  Promise.all([send(sourceAsset, depositAddress), swapScheduledHandle]);
  egressGasAmount = messageMetadata.gasBudget;

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
  const swapIdToEgressId: { [key: string]: string } = {};
  const swapEgressHandle = observeEvent(
    'swapping:SwapEgressScheduled',
    chainflipApi,
    (event) => {
      swapIdToEgressId[event.data.swapId] = event.data.egressId;
      return false;
    },
    () => swapScheduledObserved,
  );
  const egressIdToBroadcastId: { [key: string]: string } = {};
  const ccmBroadcastHandle = observeEvent(
    'ethereumIngressEgress:CcmBroadcastRequested',
    chainflipApi,
    (event) => {
      egressIdToBroadcastId[event.data.egressId] = event.data.broadcastId;
      return false;
    },
    () => swapScheduledObserved,
  );
  await send(sourceAsset, depositAddress);

  const {
    data: { swapId },
  } = await swapScheduledHandle;

  while (
    !(
      swapId in swapIdToEgressAmount &&
      swapId in swapIdToEgressId &&
      swapIdToEgressId[swapId] in egressIdToBroadcastId
    )
  ) {
    await sleep(3000);
  }
  swapScheduledObserved = true;
  await Promise.all([swapExecutedHandle, swapEgressHandle, ccmBroadcastHandle]);

  egressGasAmount =
    sourceAsset !== Assets.ETH
      ? Number(swapIdToEgressAmount[swapId].replace(/,/g, ''))
      : messageMetadata.gasBudget;

  const ethTrackedData = (
    await observeEvent('ethereumChainTracking:ChainStateUpdated', chainflipApi)
  ).data.newChainState.trackedData;

  const baseFee = Number(ethTrackedData.baseFee.replace(/,/g, ''));
  const priorityFee = Number(ethTrackedData.priorityFee.replace(/,/g, ''));

  // Standard gas estimation => In the statechain we might do a less conservative estimation, otherwise
  // a good amount of gas might end up being unused (gasLimit too low).
  const maxFeePerGas = 2 * baseFee + priorityFee;

  // Max Gas Limit budget
  const gasLimitBudget = egressGasAmount / maxFeePerGas;

  console.log(
    `${tag} egressGasAmount: ${egressGasAmount}, baseFee: ${baseFee}, priorityFee: ${priorityFee}, gasLimitBudget: ${gasLimitBudget}`,
  );
  console.log('total gas limit needed: ', gasConsumption + GAS_OVERHEAD);

  // Get the gasToConsume either decoding the message or returning it in gasTestCcmMetadata
  if (gasConsumption + GAS_OVERHEAD /* + probably some gasLimit margin */ >= gasLimitBudget) {
    // Expect Broadcast Aborted
    await observeEvent(
      'ethereumBroadcaster:BroadcastAborted',
      await getChainflipApi(),
      (event) => event.data.broadcastId === egressIdToBroadcastId[swapIdToEgressId[swapId]],
    );
    stopObservingCcmReceived = true;
    if ((await ccmReceived) !== undefined) {
      throw new Error(`${tag} CCM event emitted. Transaction should not have been broadcasted!`);
    }
  } else if ((await ccmReceived)?.returnValues.ccmTestGasUsed < gasConsumption) {
    throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
  }
}

// Spamming to raise Ethereum's fee, otherwise it will get stuck at almost zero fee. For some reason the base fee
// is not going up but the priority fee goes from 0 to 10**9.
let spam = true;
async function spamEthereum() {
  while (spam) {
    sendEth('0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266', '1');
    await sleep(1000);
  }
}

// NOTE: In localnet the gasPrice is is extremely low (~7wei) so the gasBudget needed is very small.
export async function testGasLimitCcmSwaps() {
  console.log('=== Testing GasLimit CCM swaps ===');

  // Spam ethereum with transfers to increase the gas price
  const spamming = spamEthereum();

  const gasLimitSwapsSufBudget = [
    testGasLimitSwap('DOT', 'FLIP'),
    testGasLimitSwap('ETH', 'USDC'),
    testGasLimitSwap('FLIP', 'ETH'),
    testGasLimitSwap('BTC', 'ETH'),
  ];

  // This amount of gas will be swapped into not enough destination gas. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap intput amount. So for now we provide the minimum so it gets swapped to just > 0.
  const gasLimitSwapsInsufBudget = [
    testGasLimitSwap('DOT', 'FLIP', undefined, 10 ** 6),
    testGasLimitSwap('ETH', 'USDC', undefined, 10 ** 8),
    testGasLimitSwap('FLIP', 'ETH', undefined, 10 ** 6),
    testGasLimitSwap('BTC', 'ETH', undefined, 10 ** 4),
  ];

  // As of now this is broadcasted regardless of the gas budget and even when the final solution is implemented
  // this should be broadcasted, since the gas budget should be enough, since by default gasBudget is 1% of the
  // principal and the gasPrice is very low in localnet (~7wei).
  const ccmgasLimitSwapsDefault = [
    testSwap(
      'DOT',
      'FLIP',
      undefined,
      gasTestCcmMetadata('DOT'),
      tagSuffix + ' SufficientGasBudget',
    ),
    testSwap(
      'ETH',
      'USDC',
      undefined,
      gasTestCcmMetadata('ETH'),
      tagSuffix + ' SufficientGasBudget',
    ),
    testSwap(
      'FLIP',
      'ETH',
      undefined,
      gasTestCcmMetadata('FLIP'),
      tagSuffix + ' SufficientGasBudget',
    ),
    testSwap(
      'BTC',
      'ETH',
      undefined,
      gasTestCcmMetadata('BTC'),
      tagSuffix + ' SufficientGasBudget',
    ),
  ];

  await Promise.all([
    ...gasLimitSwapsSufBudget,
    ...gasLimitSwapsInsufBudget,
    ...ccmgasLimitSwapsDefault,
  ]);

  spam = false;
  await spamming;

  console.log('=== GasLimit CCM test completed ===');
}
