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
import { signAndSendTxEth } from './send_eth';

// This test uses the CFTester contract as the receiver for a CCM call. The contract will consume approximately
// the gasLimitBudget amount specified in the CCM message with an error margin. On top of that, the gasLimitBudget overhead of the
// CCM call itself is ~115k with some variability depending on the parameters. We also add extra gasLimitBudget depending
// on the lenght of the message.
const MIN_BASE_GAS_OVERHEAD = 100000;
const BASE_GAS_OVERHEAD_BUFFER = 20000;
const CFE_GAS_LIMIT_CAP = 10000000;
// Arbitrary gas consumption values for testing. The total default gas used is then ~360-380k depending on the parameters.
let DEFAULT_GAS_CONSUMPTION = 260000;
const MIN_TEST_GAS_CONSUMPTION = 200000;
const MAX_TEST_GAS_CONSUMPTION = 4000000;
// The base overhead increases with message lenght. This is an approximation => BASE_GAS_OVERHEAD + messageLength * gasPerByte
// EVM requires 16 gas per calldata byte so a reasonable approximation is 17 to cover hashing and other operations over the data.
const GAS_PER_BYTE = 17;
const MIN_PRIORITY_FEE = 1000000000;
const LOOP_TIMEOUT = 15;

let stopObservingCcmReceived = false;

function gasTestCcmMetadata(sourceAsset: Asset, gasToConsume: number, gasBudgetFraction?: number) {
  const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');

  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasToConsume]),
    gasBudgetFraction,
  );
}

async function getChainFees() {
  const chainflipApi = await getChainflipApi();

  const ethTrackedData = (
    await observeEvent('ethereumChainTracking:ChainStateUpdated', chainflipApi)
  ).data.newChainState.trackedData;

  const baseFee = Number(ethTrackedData.baseFee.replace(/,/g, ''));
  const priorityFee = Number(ethTrackedData.priorityFee.replace(/,/g, ''));
  return { baseFee, priorityFee };
}

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  testTag?: string,
  gasToConsume?: number,
  gasBudgetFraction?: number,
  addressType?: BtcAddressType,
) {
  const chainflipApi = await getChainflipApi();

  // Increase the gas consumption to make sure all the messages are unique
  const gasConsumption = gasToConsume ?? DEFAULT_GAS_CONSUMPTION++;

  const messageMetadata = gasTestCcmMetadata(sourceAsset, gasConsumption, gasBudgetFraction);
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    addressType,
    messageMetadata,
    ` GasLimit${testTag || ''}`,
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

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const broadcastIdToTxPayload: { [key: string]: any } = {};
  const broadcastRequesthandle = observeEvent(
    'ethereumBroadcaster:TransactionBroadcastRequest',
    chainflipApi,
    (event) => {
      broadcastIdToTxPayload[event.data.broadcastId] = event.data.transactionPayload;
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
      swapIdToEgressId[swapId] in egressIdToBroadcastId &&
      egressIdToBroadcastId[swapIdToEgressId[swapId]] in broadcastIdToTxPayload
    )
  ) {
    await sleep(3000);
  }
  swapScheduledObserved = true;
  await Promise.all([
    swapExecutedHandle,
    swapEgressHandle,
    ccmBroadcastHandle,
    broadcastRequesthandle,
  ]);

  const egressBudgetAmount =
    sourceAsset !== Assets.ETH
      ? Number(swapIdToEgressAmount[swapId].replace(/,/g, ''))
      : messageMetadata.gasBudget;

  const txPayload = broadcastIdToTxPayload[egressIdToBroadcastId[swapIdToEgressId[swapId]]];
  const maxFeePerGas = Number(txPayload.maxFeePerGas.replace(/,/g, ''));
  const gasLimitBudget = Number(txPayload.gasLimit.replace(/,/g, ''));

  const byteLength = Web3.utils.hexToBytes(messageMetadata.message).length;

  const minGasLimitRequired = gasConsumption + MIN_BASE_GAS_OVERHEAD + byteLength * GAS_PER_BYTE;

  // This is a very rough approximation for the gas limit required. A buffer is added to account for that.
  if (minGasLimitRequired + BASE_GAS_OVERHEAD_BUFFER >= gasLimitBudget) {
    observeCcmReceived(
      sourceAsset,
      destAsset,
      destAddress,
      messageMetadata,
      undefined,
      () => stopObservingCcmReceived,
    ).then((event) => {
      if (event !== undefined) {
        throw new Error(`${tag} CCM event emitted. Transaction should not have been broadcasted!`);
      }
    });
    // Expect Broadcast Aborted
    console.log(
      `${tag} Gas budget of ${gasLimitBudget} is too low. Expecting BroadcastAborted event.`,
    );
    await observeEvent(
      'ethereumBroadcaster:BroadcastAborted',
      await getChainflipApi(),
      (event) => event.data.broadcastId === egressIdToBroadcastId[swapIdToEgressId[swapId]],
    );
    stopObservingCcmReceived = true;
    console.log(
      `${tag} Broadcast Aborted found! broadcastId: ${
        egressIdToBroadcastId[swapIdToEgressId[swapId]]
      }`,
    );
  } else if (minGasLimitRequired < gasLimitBudget) {
    console.log(`${tag} Gas budget ${gasLimitBudget}. Expecting successful broadcast.`);

    const ccmReceived = await observeCcmReceived(
      sourceAsset,
      destAsset,
      destAddress,
      messageMetadata,
    );
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }

    const web3 = new Web3(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545');
    const receipt = await web3.eth.getTransactionReceipt(ccmReceived?.txHash as string);
    const tx = await web3.eth.getTransaction(ccmReceived?.txHash as string);
    const gasUsed = receipt.gasUsed;
    const gasPrice = tx.gasPrice;
    const totalFee = gasUsed * Number(gasPrice);

    const feeDeficitHandle = observeEvent(
      'ethereumBroadcaster:TransactionFeeDeficitRecorded',
      await getChainflipApi(),
      (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee,
    );

    // Priority fee is not fully deterministic so we just log it for now
    if (tx.maxFeePerGas !== maxFeePerGas.toString()) {
      throw new Error(
        `${tag} Tx Max fee per gas ${tx.maxFeePerGas} different than expected ${maxFeePerGas}`,
      );
    }
    if (tx.gas !== Math.min(gasLimitBudget, CFE_GAS_LIMIT_CAP)) {
      throw new Error(`${tag} Tx gas limit ${tx.gas} different than expected ${gasLimitBudget}`);
    }
    // This should not happen by definition, as maxFeePerGas * gasLimit < egressBudgetAmount
    if (totalFee > egressBudgetAmount) {
      throw new Error(`${tag} Transaction fee paid is higher than the budget paid by the user!`);
    }
    console.log(`${tag} Swap success! TxHash: ${ccmReceived?.txHash as string}!`);

    console.log(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle;
    console.log(`${tag} Fee deficit recorded!`);
  } else {
    console.log(`${tag} Budget too tight, can't determine if swap should succeed.`);
  }
}

// Spamming to raise Ethereum's fee, otherwise it will get stuck at almost zero fee (~7 wei)
let spam = true;

async function spamEthereum() {
  while (spam) {
    signAndSendTxEth(
      '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266',
      '1',
      undefined,
      undefined,
      false,
    );
    await sleep(500);
  }
}

const usedNumbers = new Set<number>();

function getRandomGasConsumption(): number {
  const range = MAX_TEST_GAS_CONSUMPTION - MIN_TEST_GAS_CONSUMPTION + 1;
  let randomInt = Math.floor(Math.random() * range) + MIN_TEST_GAS_CONSUMPTION;
  while (usedNumbers.has(randomInt)) {
    randomInt = Math.floor(Math.random() * range) + MIN_TEST_GAS_CONSUMPTION;
  }
  usedNumbers.add(randomInt);
  return randomInt;
}

export async function testGasLimitCcmSwaps() {
  // Spam ethereum with transfers to increase the gasLimitBudget price
  const spamming = spamEthereum();

  // Wait for the fees to increase to the stable expected amount
  let i = 0;
  while ((await getChainFees()).priorityFee < MIN_PRIORITY_FEE) {
    if (++i > LOOP_TIMEOUT) {
      spam = false;
      await spamming;
      console.log("=== Skipping gasLimit CCM test as the priority fee didn't increase enough. ===");
      return;
    }
    await sleep(500);
  }

  // The default gas budgets should allow for almost any reasonable gas consumption
  const gasLimitSwapsDefault = [
    testGasLimitSwap('DOT', 'FLIP', undefined, getRandomGasConsumption()),
    testGasLimitSwap('ETH', 'USDC', undefined, getRandomGasConsumption()),
    testGasLimitSwap('FLIP', 'ETH', undefined, getRandomGasConsumption()),
    testGasLimitSwap('BTC', 'ETH', undefined, getRandomGasConsumption()),
  ];

  // reducing gas budget input amount used for gas to achieve a gasLimitBudget ~= 4-500k, which is enough for the CCM broadcast.
  const gasLimitSwapsSufBudget = [
    testGasLimitSwap('DOT', 'FLIP', ' sufBudget', undefined, 750),
    testGasLimitSwap('ETH', 'USDC', ' sufBudget', undefined, 7500),
    testGasLimitSwap('FLIP', 'ETH', ' sufBudget', undefined, 6000),
    testGasLimitSwap('BTC', 'ETH', ' sufBudget', undefined, 750),
  ];

  // None of this should be broadcasted as the gasLimitBudget is not enough
  const gasLimitSwapsInsufBudget = [
    testGasLimitSwap('DOT', 'FLIP', ' insufBudget', undefined, 10 ** 4),
    testGasLimitSwap('ETH', 'USDC', ' insufBudget', undefined, 10 ** 5),
    testGasLimitSwap('FLIP', 'ETH', ' insufBudget', undefined, 10 ** 5),
    testGasLimitSwap('BTC', 'ETH', ' insufBudget', undefined, 10 ** 4),
  ];

  // This amount of gasLimitBudget will be swapped into very little gasLimitBudget. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap intput amount. So for now we provide the minimum so it gets swapped to just > 0.
  const gasLimitSwapsNoBudget = [
    testGasLimitSwap('DOT', 'FLIP', ' noBudget', undefined, 10 ** 6),
    testGasLimitSwap('ETH', 'USDC', ' noBudget', undefined, 10 ** 8),
    testGasLimitSwap('FLIP', 'ETH', ' noBudget', undefined, 10 ** 6),
    testGasLimitSwap('BTC', 'ETH', ' noBudget', undefined, 10 ** 5),
  ];

  await Promise.all([
    ...gasLimitSwapsSufBudget,
    ...gasLimitSwapsInsufBudget,
    ...gasLimitSwapsDefault,
    ...gasLimitSwapsNoBudget,
  ]);

  spam = false;
  await spamming;

  // Make sure all the spamming has stopped to avoid triggering connectivity issues when running the next test.
  await sleep(10000);
}
