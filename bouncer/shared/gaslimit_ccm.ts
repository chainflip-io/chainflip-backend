import Web3 from 'web3';
import { Asset, Assets } from '@chainflip-io/cli';
import { newCcmMetadata, prepareSwap } from './swapping';
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
import { signAndSendTxEthSilent } from './send_eth';

// TODO: We should probably put a cap on the message length to avoid issues. However CF will still not lose money
// as the user will be paying for the entire transfer anyway => example of a message length limit could be 10000 bytes.
// Maybe we can raise it a bit more just in case (e.g 15k) but just setting some limit to avoid being DoSed.

// This test uses the CFTester contract as the receiver for a CCM call. The contract will consume approximately
// the gasLimitBudget amount specified in the CCM message with an error margin. On top of that, the gasLimitBudget overhead of the
// CCM call itself is ~115k with some variability depending on the parameters. We also add extra gasLimitBudget depending
// on the lenght of the message. Up until 260k gasLimitBudget spent on the test it should succeed, more than that it can fail.
// 260k + 120k ~= 380k
const BASE_GAS_OVERHEAD = 120000;
// 270k also works in almost some cases but in some scenarios it might be just above the threshold
// TODO: This is an arbitrary value used because atm we have a 400k hardcoded limit. Can be modified when we confirm this is working.
const DEFAULT_GAS_CONSUMPTION = 260000;
// The base overhead increases with message lenght => BASE_GAS_OVERHEAD + messageLength * gasPerByte
const GAS_PER_BYTE = 16;

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
  tagSuffix?: string,
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
    ' GasLimit ' + tagSuffix,
  );

  const ccmReceivedHandle = observeCcmReceived(
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
    undefined,
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
  // instead. In any other scenario, including when destAsset is ETH, both principal and gasLimitBudget are being swapped.
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

  const egressBudgetAmount =
    sourceAsset !== Assets.ETH
      ? Number(swapIdToEgressAmount[swapId].replace(/,/g, ''))
      : messageMetadata.gasBudget;

  const ethTrackedData = (
    await observeEvent('ethereumChainTracking:ChainStateUpdated', chainflipApi)
  ).data.newChainState.trackedData;

  const baseFee = Number(ethTrackedData.baseFee.replace(/,/g, ''));
  const priorityFee = Number(ethTrackedData.priorityFee.replace(/,/g, ''));

  // Standard gasLimitBudget estimation for now.
  const maxFeePerGas = 2 * baseFee + priorityFee;

  // On the state chain the gasLimit is calculated from the egressBudget and the MmaxFeePerGas
  const gasLimitBudget = egressBudgetAmount / maxFeePerGas;

  const byteLength = Web3.utils.hexToBytes(messageMetadata.message).length;

  console.log(
    `${tag} egressBudgetAmount: ${egressBudgetAmount}, baseFee: ${baseFee}, priorityFee: ${priorityFee}, gasLimitBudget: ${gasLimitBudget}`,
  );
  console.log('extra gasLimitBudget', byteLength * GAS_PER_BYTE);
  // This is a rough approximation, will be slightly different for each swap/egressToken/message
  console.log(
    'total gasLimitBudget limit needed: ',
    gasConsumption +
      BASE_GAS_OVERHEAD +
      byteLength * GAS_PER_BYTE /* + probably some gasLimit margin */,
  );

  // This is a very rough approximation as there might be extra overhead in the logic but it's probably good
  // enough for testing if we add some margins around the gasBudget cutoff.
  if (
    gasConsumption +
      BASE_GAS_OVERHEAD +
      byteLength * GAS_PER_BYTE /* + probably some gasLimit margin */ >=
    gasLimitBudget
  ) {
    console.log(`${tag} Gas budget is too low. Expecting BroadcastAborted event.`);
    // Expect Broadcast Aborted
    await observeEvent(
      'ethereumBroadcaster:BroadcastAborted',
      await getChainflipApi(),
      (event) => event.data.broadcastId === egressIdToBroadcastId[swapIdToEgressId[swapId]],
    );
    stopObservingCcmReceived = true;
    if ((await ccmReceivedHandle) !== undefined) {
      throw new Error(`${tag} CCM event emitted. Transaction should not have been broadcasted!`);
    }
  } else {
    const ccmReceived = await ccmReceivedHandle;
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }
    const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
    const receipt = await web3.eth.getTransactionReceipt(ccmReceived?.txHash as string);
    const tx = await web3.eth.getTransaction(ccmReceived?.txHash as string);
    const gasUsed = receipt.gasUsed;
    const gasPrice = tx.gasPrice;
    const totalFee = gasUsed * Number(gasPrice);
    const percBudgetUsed = (totalFee * 100) / egressBudgetAmount;
    const percGasUsed = (gasUsed * 100) / gasLimitBudget;
    console.log(tag);
    console.log('gasLimitBudget     ', gasLimitBudget);
    console.log('gasUsed            ', gasUsed);
    console.log('maxFeePerGas       ', maxFeePerGas);
    console.log('gasPrice           ', gasPrice);
    console.log('totalFee           ', totalFee);
    console.log('egressBudgetAmount ', egressBudgetAmount);
    console.log(`percBudgetUsed     ${percBudgetUsed}%`);
    console.log(`percGasUsed        ${percGasUsed}%`);
    // This should not happen by definition, as maxFeePerGas * gasLimit < egressBudgetAmount
    if (totalFee > egressBudgetAmount) {
      throw new Error(`${tag} Transaction fee paid is higher than the budget paid by the user!`);
    }
  }
}

// Spamming to raise Ethereum's fee, otherwise it will get stuck at almost zero fee. For some reason the base fee
// is not going up but the priority fee goes from 0 to 10**9.
let spam = true;
async function spamEthereum() {
  while (spam) {
    signAndSendTxEthSilent('0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266', '1');
    await sleep(500);
  }
}

// NOTE: In localnet the gasPrice is is extremely low (~7wei) so the gasBudget needed is very small.
export async function testGasLimitCcmSwaps() {
  console.log('=== Testing GasLimit CCM swaps ===');

  // Spam ethereum with transfers to increase the gasLimitBudget price
  const spamming = spamEthereum();

  // TODO: Add some test with a long enough message that we shouldn't be broadcasting beacuse user hasn't paid enough gasLimitBudget.
  // E.g. Gas = 120k (overhead) + 10000 bytes * 16 gasLimitBudget/byte = 280k gasLimitBudget => Pay less than that as the fee.

  const gasLimitSwapsDefault = [
    // testGasLimitSwap('DOT', 'FLIP'),
    // testGasLimitSwap('ETH', 'USDC'),
    // testGasLimitSwap('FLIP', 'ETH'),
    // testGasLimitSwap('BTC', 'ETH'),
  ];

  const gasLimitSwapsSufBudget = [
    testGasLimitSwap('DOT', 'FLIP', 'sufBudget', undefined, 10 ** 3), // % 10 ~ gasLimitBudget ~= 867k
    testGasLimitSwap('ETH', 'USDC', 'sufBudget', undefined, 10 ** 4), // % 100 ~ gasLimitBudget ~= 499k
    testGasLimitSwap('FLIP', 'ETH', 'sufBudget', undefined, 10 ** 3), // % 10 ~ gasLimitBudget ~= 389k
    testGasLimitSwap('BTC', 'ETH', 'sufBudget', undefined, 10 ** 3), // % 10 ~ gasLimitBudget ~= 862k
  ];

  const gasLimitSwapsInsufBudget = [
    // None of this should be broadcasted as the gasLimitBudget is not enough
    // testGasLimitSwap('DOT', 'FLIP', 'insufBudget', undefined, 10 ** 4),
    // testGasLimitSwap('ETH', 'USDC', 'insufBudget', undefined, 10 ** 5),
    // testGasLimitSwap('FLIP', 'ETH', 'insufBudget', undefined, 10 ** 4),
    // testGasLimitSwap('BTC', 'ETH', 'insufBudget', undefined, 10 ** 4),
  ];

  // This amount of gasLimitBudget will be swapped into not enough destination gasLimitBudget. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap intput amount. So for now we provide the minimum so it gets swapped to just > 0.
  const gasLimitSwapsNoBudget = [
    // testGasLimitSwap('DOT', 'FLIP', 'noBudget', undefined, 10 ** 6),
    // testGasLimitSwap('ETH', 'USDC', 'noBudget', undefined, 10 ** 8),
    // testGasLimitSwap('FLIP', 'ETH', 'noBudget', undefined, 10 ** 6),
    // testGasLimitSwap('BTC', 'ETH', 'noBudget', undefined, 10 ** 4),
  ];

  await Promise.all([
    ...gasLimitSwapsSufBudget,
    ...gasLimitSwapsInsufBudget,
    ...gasLimitSwapsDefault,
    ...gasLimitSwapsNoBudget,
  ]);

  spam = false;
  await spamming;

  console.log('=== GasLimit CCM test completed ===');
}
