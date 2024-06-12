import Web3 from 'web3';
import { InternalAsset as Asset, InternalAssets as Assets, Chain } from '@chainflip/cli';
import { newCcmMetadata, prepareSwap } from './swapping';
import {
  chainFromAsset,
  chainGasAsset,
  getEvmEndpoint,
  observeCcmReceived,
  observeSwapScheduled,
  sleep,
  SwapType,
} from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { spamEvm } from './send_evm';
import { observeEvent, observeBadEvent } from './utils/substrate';

// This test uses the CFTester contract as the receiver for a CCM call. The contract will consume approximately
// the gasLimitBudget amount specified in the CCM message with an error margin. On top of that, the gasLimitBudget overhead of the
// CCM call itself is ~115k (Eth) ~5.2M (Arb) with some variability depending on the parameters. We also add extra gasLimitBudget
// depending on the lenght of the message.
const MIN_BASE_GAS_OVERHEAD: Record<string, number> = { Ethereum: 100000, Arbitrum: 5200000 };
const BASE_GAS_OVERHEAD_BUFFER: Record<string, number> = { Ethereum: 20000, Arbitrum: 200000 };
const CFE_GAS_LIMIT_CAP: Record<string, number> = { Ethereum: 10000000, Arbitrum: 25000000 };
// Minimum and maximum gas consumption values to be in a useful range for testing.
const MIN_TEST_GAS_CONSUMPTION: Record<string, number> = { Ethereum: 200000, Arbitrum: 1000000 };
const MAX_TEST_GAS_CONSUMPTION: Record<string, number> = {
  Ethereum: 4000000,
  Arbitrum: 6000000,
};
// Arbitrary default gas consumption values for testing.
const DEFAULT_GAS_CONSUMPTION: Record<string, number> = { Ethereum: 260000, Arbitrum: 3000000 };
// The base overhead increases with message lenght. This is an approximation => BASE_GAS_OVERHEAD + messageLength * gasPerByte
// EVM requires 16 gas per calldata byte so a reasonable approximation is 17 to cover hashing and other operations over the data.
const GAS_PER_BYTE = 17;
// MIN_FEE is the priority fee for Ethereum and baseFee for Arbitrum, since those are the fees that increase here upon spamming.
const MIN_FEE: Record<string, number> = { Ethereum: 1000000000, Arbitrum: 100000000 };
const LOOP_TIMEOUT = 15;

function gasTestCcmMetadata(sourceAsset: Asset, gasToConsume: number, gasBudgetFraction?: number) {
  const web3 = new Web3();

  return newCcmMetadata(
    sourceAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasToConsume]),
    gasBudgetFraction,
  );
}

async function getEvmChainFees(chain: Chain) {
  const trackedData = (
    await observeEvent(`${chain.toLowerCase()}ChainTracking:ChainStateUpdated`).event
  ).data.newChainState.trackedData;

  const baseFee = Number(trackedData.baseFee.replace(/,/g, ''));

  // Arbitrum doesn't have priority fee
  let priorityFee = 0;
  if (chain !== 'Arbitrum') {
    priorityFee = Number(trackedData.priorityFee.replace(/,/g, ''));
  }

  return { baseFee, priorityFee };
}

async function testGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  testTag?: string,
  gasToConsume?: number,
  gasBudgetFraction?: number,
) {
  const destChain = chainFromAsset(destAsset).toString();

  // Increase the gas consumption to make sure all the messages are unique
  const gasConsumption = gasToConsume ?? DEFAULT_GAS_CONSUMPTION[destChain]++;

  const messageMetadata = gasTestCcmMetadata(sourceAsset, gasConsumption, gasBudgetFraction);
  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
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

  const swapScheduledHandle = observeSwapScheduled(
    sourceAsset,
    destAsset,
    channelId,
    SwapType.CcmPrincipal,
  );

  let gasSwapScheduledHandle;

  if (chainGasAsset(destChain as Chain) !== sourceAsset) {
    gasSwapScheduledHandle = observeSwapScheduled(
      sourceAsset,
      destChain === 'Ethereum' ? Assets.Eth : Assets.ArbEth,
      channelId,
      SwapType.CcmGas,
    );
  }

  // SwapExecuted is emitted at the same time as swapScheduled so we can't wait for swapId to be known.
  const swapIdToEgressAmount: { [key: string]: string } = {};
  const swapExecutedHandle = observeEvent('swapping:SwapExecuted', {
    test: (event) => {
      swapIdToEgressAmount[event.data.swapId] = event.data.egressAmount;
      return false;
    },
    abortable: true,
  });
  const swapIdToEgressId: { [key: string]: string } = {};
  const swapEgressHandle = observeEvent('swapping:SwapEgressScheduled', {
    test: (event) => {
      swapIdToEgressId[event.data.swapId] = event.data.egressId;
      return false;
    },
    abortable: true,
  });

  const egressIdToBroadcastId: { [key: string]: string } = {};
  const ccmBroadcastHandle = observeEvent(
    `${destChain.toLowerCase()}IngressEgress:CcmBroadcastRequested`,
    {
      test: (event) => {
        egressIdToBroadcastId[event.data.egressId] = event.data.broadcastId;
        return false;
      },
      abortable: true,
    },
  );

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const broadcastIdToTxPayload: { [key: string]: any } = {};
  const broadcastRequesthandle = observeEvent(
    `${destChain.toLowerCase()}Broadcaster:TransactionBroadcastRequest`,
    {
      test: (event) => {
        broadcastIdToTxPayload[event.data.broadcastId] = event.data.transactionPayload;
        return false;
      },
      abortable: true,
    },
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

  console.log(`${tag} Swap events found`);

  swapExecutedHandle.stop();
  swapEgressHandle.stop();
  ccmBroadcastHandle.stop();
  broadcastRequesthandle.stop();

  await Promise.all([
    swapExecutedHandle,
    swapEgressHandle,
    ccmBroadcastHandle,
    broadcastRequesthandle,
  ]);

  let egressBudgetAmount;

  if (chainGasAsset(destChain as Chain) === sourceAsset) {
    egressBudgetAmount = messageMetadata.gasBudget;
  } else {
    const {
      data: { swapId: gasSwapId },
    } = await gasSwapScheduledHandle!;
    egressBudgetAmount = Number(swapIdToEgressAmount[gasSwapId].replace(/,/g, ''));
  }

  const txPayload = broadcastIdToTxPayload[egressIdToBroadcastId[swapIdToEgressId[swapId]]];
  const maxFeePerGas = Number(txPayload.maxFeePerGas.replace(/,/g, ''));
  const gasLimitBudget = Number(txPayload.gasLimit.replace(/,/g, ''));

  const byteLength = Web3.utils.hexToBytes(messageMetadata.message).length;

  const minGasLimitRequired =
    gasConsumption + MIN_BASE_GAS_OVERHEAD[destChain] + byteLength * GAS_PER_BYTE;
  // This is a very rough approximation for the gas limit required. A buffer is added to account for that.
  if (minGasLimitRequired + BASE_GAS_OVERHEAD_BUFFER[destChain] >= gasLimitBudget) {
    let stopObservingCcmReceived = false;

    await observeCcmReceived(
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
    await observeEvent(`${destChain.toLowerCase()}Broadcaster:BroadcastAborted`, {
      test: (event) => event.data.broadcastId === egressIdToBroadcastId[swapIdToEgressId[swapId]],
    }).event;
    stopObservingCcmReceived = true;
    console.log(
      `${tag} Broadcast Aborted found! broadcastId: ${
        egressIdToBroadcastId[swapIdToEgressId[swapId]]
      }`,
    );
  } else if (minGasLimitRequired + BASE_GAS_OVERHEAD_BUFFER[destChain] < gasLimitBudget) {
    console.log(`${tag} Gas budget ${gasLimitBudget}. Expecting successful broadcast.`);

    // Check that broadcast is not aborted
    const observeBroadcastFailure = observeBadEvent(
      `${destChain.toLowerCase()}Broadcaster:BroadcastAborted`,
      {
        test: (event) => {
          const aborted =
            event.data.broadcastId === egressIdToBroadcastId[swapIdToEgressId[swapId]];
          if (aborted) {
            console.log(
              `${tag} FAILURE! Broadcast Aborted unexpected! broadcastId: ${
                event.data.broadcastId
              }. Gas budget: ${gasLimitBudget} while limit is ${
                minGasLimitRequired + BASE_GAS_OVERHEAD_BUFFER[destChain]
              }!`,
            );
          }
          return aborted;
        },
        label: testTag,
      },
    );

    // Expecting success
    const ccmReceived = await observeCcmReceived(
      sourceAsset,
      destAsset,
      destAddress,
      messageMetadata,
    );
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }

    // Stop listening for broadcast failure
    await observeBroadcastFailure.stop();

    const web3 = new Web3(getEvmEndpoint(chainFromAsset(destAsset)));
    const receipt = await web3.eth.getTransactionReceipt(ccmReceived?.txHash as string);
    const tx = await web3.eth.getTransaction(ccmReceived?.txHash as string);
    const gasUsed = receipt.gasUsed;
    const gasPrice = tx.gasPrice;
    const totalFee = gasUsed * Number(gasPrice);

    const feeDeficitHandle = observeEvent(
      `${destChain.toLowerCase()}Broadcaster:TransactionFeeDeficitRecorded`,
      { test: (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee },
    );

    // Priority fee is not fully deterministic so we just log it for now
    if (tx.maxFeePerGas !== maxFeePerGas.toString()) {
      throw new Error(
        `${tag} Tx Max fee per gas ${tx.maxFeePerGas} different than expected ${maxFeePerGas}`,
      );
    }
    if (tx.gas !== Math.min(gasLimitBudget, CFE_GAS_LIMIT_CAP[destChain])) {
      throw new Error(`${tag} Tx gas limit ${tx.gas} different than expected ${gasLimitBudget}`);
    }
    // This should not happen by definition, as maxFeePerGas * gasLimit < egressBudgetAmount
    if (totalFee > egressBudgetAmount) {
      throw new Error(
        `${tag} Transaction fee paid is higher than the budget paid by the user! totalFee: ${totalFee} egressBudgetAmount: ${egressBudgetAmount}`,
      );
    }
    console.log(`${tag} Swap success! TxHash: ${ccmReceived?.txHash as string}!`);

    console.log(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle.event;
    console.log(`${tag} Fee deficit recorded!`);
  } else {
    console.log(`${tag} Budget too tight, can't determine if swap should succeed.`);
  }
}

const usedNumbers = new Set<number>();

function getRandomGasConsumption(chain: Chain): number {
  const range = MAX_TEST_GAS_CONSUMPTION[chain] - MIN_TEST_GAS_CONSUMPTION[chain] + 1;
  let randomInt = Math.floor(Math.random() * range) + MIN_TEST_GAS_CONSUMPTION[chain];
  while (usedNumbers.has(randomInt)) {
    randomInt = Math.floor(Math.random() * range) + MIN_TEST_GAS_CONSUMPTION[chain];
  }
  usedNumbers.add(randomInt);
  return randomInt;
}

// Spamming to raise Ethereum's fee, otherwise it will get stuck at almost zero fee (~7 wei)
let spam = true;

export async function testGasLimitCcmSwaps() {
  // Spam chains to increase the gasLimitBudget price
  const spammingEth = spamEvm('Ethereum', 500, () => spam);
  const spammingArb = spamEvm('Arbitrum', 500, () => spam);

  // Wait for the fees to increase to the stable expected amount
  let i = 0;
  while (
    (await getEvmChainFees('Ethereum')).priorityFee < MIN_FEE.Ethereum ||
    (await getEvmChainFees('Arbitrum')).baseFee < MIN_FEE.Arbitrum
  ) {
    console.log('Arbitrum fees', await getEvmChainFees('Arbitrum'));
    console.log('Ethereum fees', await getEvmChainFees('Ethereum'));
    if (++i > LOOP_TIMEOUT) {
      spam = false;
      await spammingEth;
      await spammingArb;
      console.log("=== Skipping gasLimit CCM test as the priority fee didn't increase enough. ===");
      return;
    }
    await sleep(500);
  }

  const gasLimitSwapsDefault = [
    testGasLimitSwap('Dot', 'Flip', undefined, getRandomGasConsumption('Ethereum')),
    testGasLimitSwap('Eth', 'Usdc', undefined, getRandomGasConsumption('Ethereum')),
    testGasLimitSwap('Eth', 'Usdt', undefined, getRandomGasConsumption('Ethereum')),
    testGasLimitSwap('Flip', 'Eth', undefined, getRandomGasConsumption('Ethereum')),
    testGasLimitSwap('Btc', 'Eth', undefined, getRandomGasConsumption('Ethereum')),
    testGasLimitSwap('Dot', 'ArbEth', undefined, getRandomGasConsumption('Arbitrum')),
    testGasLimitSwap('Eth', 'ArbUsdc', undefined, getRandomGasConsumption('Arbitrum')),
    testGasLimitSwap('Flip', 'ArbEth', undefined, getRandomGasConsumption('Arbitrum')),
    testGasLimitSwap('ArbEth', 'Eth', undefined, getRandomGasConsumption('Arbitrum')),
  ];

  // Gas budget to 10% of the default swap amount, which should be enough
  const gasLimitSwapsSufBudget = [
    testGasLimitSwap('Dot', 'Usdc', ' sufBudget', undefined, 10),
    testGasLimitSwap('Usdc', 'Eth', ' sufBudget', undefined, 10),
    testGasLimitSwap('Flip', 'Usdt', ' sufBudget', undefined, 10),
    testGasLimitSwap('Usdt', 'Eth', ' sufBudget', undefined, 10),
    testGasLimitSwap('Btc', 'Flip', ' sufBudget', undefined, 10),
    testGasLimitSwap('Dot', 'ArbEth', ' sufBudget', undefined, 10),
    testGasLimitSwap('Eth', 'ArbUsdc', ' sufBudget', undefined, 10),
    testGasLimitSwap('ArbEth', 'Flip', ' sufBudget', undefined, 10),
    testGasLimitSwap('Btc', 'ArbUsdc', ' sufBudget', undefined, 10),
    testGasLimitSwap('Eth', 'ArbEth', ' sufBudget', undefined, 10),
    testGasLimitSwap('ArbUsdc', 'Flip', ' sufBudget', undefined, 10),
  ];

  // This amount of gasLimitBudget will be swapped into very little gasLimitBudget. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap input amount. So for now we provide the minimum so it gets swapped to just > 0.
  const gasLimitSwapsInsufBudget = [
    testGasLimitSwap('Dot', 'Flip', ' insufBudget', undefined, 10 ** 6),
    testGasLimitSwap('Eth', 'Usdc', ' insufBudget', undefined, 10 ** 8),
    testGasLimitSwap('Eth', 'Usdt', ' insufBudget', undefined, 10 ** 8),
    testGasLimitSwap('Flip', 'Eth', ' insufBudget', undefined, 10 ** 6),
    testGasLimitSwap('Btc', 'Eth', ' insufBudget', undefined, 10 ** 5),
    testGasLimitSwap('Dot', 'ArbUsdc', ' insufBudget', undefined, 10 ** 6),
    testGasLimitSwap('Eth', 'ArbEth', ' insufBudget', undefined, 10 ** 8),
    testGasLimitSwap('Flip', 'ArbUsdc', ' insufBudget', undefined, 10 ** 6),
    testGasLimitSwap('Btc', 'ArbEth', ' insufBudget', undefined, 10 ** 5),
    testGasLimitSwap('ArbEth', 'Eth', ' insufBudget', undefined, 10 ** 6),
    testGasLimitSwap('ArbUsdc', 'Flip', ' insufBudget', undefined, 10 ** 5),
  ];

  await Promise.all([
    ...gasLimitSwapsSufBudget,
    ...gasLimitSwapsDefault,
    ...gasLimitSwapsInsufBudget,
  ]);

  spam = false;
  await spammingEth;
  await spammingArb;

  // Make sure all the spamming has stopped to avoid triggering connectivity issues when running the next test.
  await sleep(10000);
}
