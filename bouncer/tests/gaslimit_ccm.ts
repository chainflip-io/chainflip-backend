import Web3 from 'web3';
import { PublicKey } from '@solana/web3.js';
import { InternalAsset as Asset, Chain } from '@chainflip/cli';
import { newCcmMetadata, prepareSwap } from '../shared/swapping';
import {
  ccmSupportedChains,
  chainFromAsset,
  chainGasAsset,
  EgressId,
  getEvmEndpoint,
  getSolConnection,
  observeCcmReceived,
  observeSwapRequested,
  sleep,
  SwapRequestType,
  SwapType,
} from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { send } from '../shared/send';
import { spamEvm } from '../shared/send_evm';
import { observeEvent, observeBadEvent, getChainflipApi } from '../shared/utils/substrate';
import { CcmDepositMetadata } from '../shared/new_swap';
import { spamSolana } from '../shared/send_sol';
import { ExecutableTest } from '../shared/executable_test';

// Run this test separately from all the concurrent tests because there will be BroadcastAborted events emitted.
/* eslint-disable @typescript-eslint/no-use-before-define */
export const testGasLimitCcmSwaps = new ExecutableTest('Gas-Limit-Ccm-Swaps', main, 1800);

const LOOP_TIMEOUT = 15;
const LAMPORTS_PER_SIGNATURE = 5000;
// Arbitrary default gas consumption values for testing.
const DEFAULT_GAS_CONSUMPTION: Record<string, number> = { Ethereum: 260000, Arbitrum: 3000000 };
// The base overhead increases with message lenght. This is an approximation => BASE_GAS_OVERHEAD + messageLength * gasPerByte
// EVM requires 16 gas per calldata byte so a reasonable approximation is 17 to cover hashing and other operations over the data.
const EVM_GAS_PER_BYTE = 17;

// After the swap is complete, we search for the expected swap event in this many past blocks.
const CHECK_PAST_BLOCKS_FOR_EVENTS = 30;

function getMinGasOverhead(chain: Chain): number {
  switch (chain) {
    case 'Ethereum':
      return 100000;
    case 'Arbitrum':
      return 5200000;
    case 'Solana':
      return 80000;
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
}

function getEngineBroadcastLimit(chain: Chain): number {
  switch (chain) {
    case 'Ethereum':
      return 10000000;
    case 'Arbitrum':
      return 25000000;
    case 'Solana':
      return 14000000;
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
}

function getBaseGasOverheadBuffer(chain: Chain): number {
  switch (chain) {
    case 'Ethereum':
      return 20000;
    case 'Arbitrum':
      return 200000;
    case 'Solana':
      return 50000;
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
}

// MIN_FEE is the priority fee for Ethereum and baseFee for Arbitrum, since those are the fees that increase here upon spamming.
function getChainMinFee(chain: Chain): number {
  switch (chain) {
    case 'Ethereum':
      return 1000000000;
    case 'Arbitrum':
      return 100000000;
    case 'Solana':
      return 100000000;
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
}

async function getChainFees(chain: Chain) {
  let baseFee = 0;
  let priorityFee = 0;

  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum': {
      const trackedData = (
        await observeEvent(`${chain.toLowerCase()}ChainTracking:ChainStateUpdated`).event
      ).data.newChainState.trackedData;
      baseFee = Number(trackedData.baseFee.replace(/,/g, ''));

      if (chain === 'Ethereum') {
        priorityFee = Number(trackedData.priorityFee.replace(/,/g, ''));
      }
      break;
    }
    case 'Solana': {
      await using chainflip = await getChainflipApi();
      const trackedData = await chainflip.query.solanaElections.electoralUnsynchronisedState();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      priorityFee = Number((trackedData.toJSON() as any[])[1].toString().replace(/,/g, ''));
      break;
    }
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
  return { baseFee, priorityFee };
}

async function trackGasLimitSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  messageMetadata: CcmDepositMetadata,
  testTag?: string,
) {
  const destChain = chainFromAsset(destAsset);
  if (!ccmSupportedChains.includes(destChain)) {
    throw new Error(`Chain ${destChain} is not supported for CCM`);
  }

  const { destAddress, tag } = await prepareSwap(
    sourceAsset,
    destAsset,
    undefined,
    messageMetadata,
    `GasLimit${testTag || ''}`,
  );

  // Do the swap
  const { depositAddress, channelId } = await requestNewSwap(
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
  );
  const swapRequestedHandle = observeSwapRequested(
    sourceAsset,
    destAsset,
    channelId,
    SwapRequestType.Ccm,
  );
  await send(sourceAsset, depositAddress);
  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));

  // Find all of the swap events
  const egressId = (
    await observeEvent('swapping:SwapEgressScheduled', {
      test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.egressId as EgressId;
  testGasLimitCcmSwaps.debugLog(`${tag} Found egressId: ${egressId}`);

  const broadcastId = (
    await observeEvent(`${destChain.toLowerCase()}IngressEgress:CcmBroadcastRequested`, {
      test: (event) =>
        event.data.egressId[0] === egressId[0] && event.data.egressId[1] === egressId[1],
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.broadcastId;
  testGasLimitCcmSwaps.debugLog(`${tag} Found broadcastId: ${broadcastId}`);

  const txPayload = (
    await observeEvent(`${destChain.toLowerCase()}Broadcaster:TransactionBroadcastRequest`, {
      test: (event) => event.data.broadcastId === broadcastId,
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.transactionPayload;
  testGasLimitCcmSwaps.debugLog(`${tag} Found txPayload: ${txPayload}`);

  // Only look for a gas swap if we expect one
  async function lookForGasSwapAmount(): Promise<number> {
    const gasSwapId = (
      await observeEvent('swapping:SwapScheduled', {
        test: (event) =>
          Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId &&
          event.data.swapType === SwapType.CcmGas,
        historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
      }).event
    ).data.swapId;

    return (
      await observeEvent('swapping:SwapExecuted', {
        test: (event) => event.data.swapId === gasSwapId,
        historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
      }).event
    ).data.outputAmount;
  }

  const egressBudgetAmount =
    chainGasAsset(destChain as Chain) === sourceAsset
      ? messageMetadata.gasBudget
      : await lookForGasSwapAmount();

  return { tag, destAddress, egressBudgetAmount, broadcastId, txPayload };
}

async function testGasLimitSwapToSolana(
  sourceAsset: Asset,
  destAsset: Asset,
  testTag?: string,
  gasBudgetFraction?: number,
) {
  const destChain = chainFromAsset(destAsset);

  if (destChain !== 'Solana') {
    throw new Error(`Destination chain ${destChain} is not Solana`);
  }

  const ccmMetadata = newCcmMetadata(sourceAsset, destAsset, undefined, gasBudgetFraction);

  const { tag, destAddress, egressBudgetAmount } = await trackGasLimitSwap(
    sourceAsset,
    destAsset,
    ccmMetadata,
    testTag,
  );

  testGasLimitCcmSwaps.log(`${tag} Finished tracking events`);

  const { priorityFee: computePrice } = await getChainFees('Solana');

  if (computePrice === 0) {
    throw new Error('Compute price should not be 0');
  }
  const gasLimitBudget = Math.floor(
    (Math.max(0, egressBudgetAmount - LAMPORTS_PER_SIGNATURE) * 10 ** 6) / computePrice,
  );

  const minGasLimitRequired = getMinGasOverhead('Solana');
  const solanaBaseComputeOverHead = getBaseGasOverheadBuffer(destChain);
  const connection = getSolConnection();

  if (minGasLimitRequired >= gasLimitBudget + solanaBaseComputeOverHead) {
    testGasLimitCcmSwaps.log(`${tag} Gas too low, transaction expected to revert`);
    let confirmedSignaturesInfo;
    let attempts = 0;
    let transaction;

    while (attempts < 20) {
      confirmedSignaturesInfo = await connection.getSignaturesForAddress(
        new PublicKey(destAddress),
        undefined,
        'finalized',
      );
      if (confirmedSignaturesInfo.length > 0) {
        if (confirmedSignaturesInfo.length > 1) {
          throw new Error('More than 1 signature found');
        }
        transaction = await connection.getTransaction(confirmedSignaturesInfo[0].signature, {
          commitment: 'confirmed',
        });
        if (transaction !== null) {
          testGasLimitCcmSwaps.log(`${tag} Transaction found: ${transaction?.meta}`);
          // This doesn't throw an error, for now it's fine to print it.
          if (transaction?.meta?.err === null) {
            throw new Error('Transaction should have reverted');
          }
          break;
        }
        await sleep(5000);
        attempts++;
      }
    }

    if (transaction === null) {
      throw new Error('Transaction not found');
    }
    testGasLimitCcmSwaps.log(`${tag} CCM Swap success!`);
  } else if (minGasLimitRequired + solanaBaseComputeOverHead < gasLimitBudget) {
    testGasLimitCcmSwaps.log(
      `${tag} Gas budget ${gasLimitBudget}. Expecting successful broadcast.`,
    );

    const ccmEvent = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
    const txSignature = ccmEvent?.txHash as string;
    testGasLimitCcmSwaps.log(`${tag} CCM event emitted!`);

    const transaction = await connection.getTransaction(txSignature, {
      commitment: 'confirmed',
    });
    // Checking that the compute limit is set correctly (and < MAX_CAP) is cumbersome without manually parsing instructions
    const totalFee =
      transaction?.meta?.fee ??
      (() => {
        throw new Error('Transaction, meta, or fee is null or undefined');
      })();
    if (transaction?.meta?.err !== null) {
      throw new Error(`${tag} Transaction should not have reverted!`);
    }
    const feeDeficitHandle = observeEvent(
      `${destChain.toLowerCase()}Broadcaster:TransactionFeeDeficitRecorded`,
      { test: (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee },
    );

    if (totalFee > egressBudgetAmount) {
      throw new Error(
        `${tag} Transaction fee paid is higher than the budget paid by the user! totalFee: ${totalFee} egressBudgetAmount: ${egressBudgetAmount}`,
      );
    }
    testGasLimitCcmSwaps.log(`${tag} CCM Swap success! TxHash: ${txSignature}!`);
    testGasLimitCcmSwaps.log(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle.event;
    testGasLimitCcmSwaps.log(`${tag} Fee deficit recorded!`);
  } else {
    testGasLimitCcmSwaps.log(`${tag} Budget too tight, can't determine if swap should succeed.`);
  }
}

const usedNumbers = new Set<number>();
// Minimum and maximum gas consumption values to be in a useful range for testing.
const MIN_TEST_GAS_CONSUMPTION: Record<string, number> = { Ethereum: 200000, Arbitrum: 1000000 };
const MAX_TEST_GAS_CONSUMPTION: Record<string, number> = {
  Ethereum: 4000000,
  Arbitrum: 6000000,
};

async function testGasLimitSwapToEvm(
  sourceAsset: Asset,
  destAsset: Asset,
  testTag?: string,
  gasBudgetFraction?: number,
  gasToConsume?: number,
) {
  const destChain = chainFromAsset(destAsset);

  if (destChain !== 'Arbitrum' && destChain !== 'Ethereum') {
    throw new Error(`Destination chain ${destChain} is not Ethereum nor Arbitrum`);
  }

  // Increase the gas consumption to make sure all the messages are unique
  const gasConsumption = gasToConsume ?? DEFAULT_GAS_CONSUMPTION[destChain.toString()]++;

  const web3 = new Web3(getEvmEndpoint(chainFromAsset(destAsset)));

  const ccmMetadata = newCcmMetadata(
    sourceAsset,
    destAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasConsumption]),
    gasBudgetFraction,
  );

  const { tag, destAddress, egressBudgetAmount, broadcastId, txPayload } = await trackGasLimitSwap(
    sourceAsset,
    destAsset,
    ccmMetadata,
    testTag,
  );
  testGasLimitCcmSwaps.log(`${tag} Finished tracking events`);

  const maxFeePerGas = Number(txPayload.maxFeePerGas.replace(/,/g, ''));
  const gasLimitBudget = Number(txPayload.gasLimit.replace(/,/g, ''));
  const byteLength = Web3.utils.hexToBytes(ccmMetadata.message).length;
  const minGasLimitRequired =
    gasConsumption + getMinGasOverhead(destChain) + byteLength * EVM_GAS_PER_BYTE;

  const evmBaseComputeOverHead = getBaseGasOverheadBuffer(destChain);
  // This is a very rough approximation for the gas limit required. A buffer is added to account for that.

  if (minGasLimitRequired >= gasLimitBudget + evmBaseComputeOverHead) {
    let stopObservingCcmReceived = false;

    testGasLimitCcmSwaps.log(
      `${tag} Gas budget of ${gasLimitBudget} is too low. Expecting BroadcastAborted event. Time to wait for CCM event`,
    );

    // We run this because we want to ensure that we *don't* get a CCM event.
    // So awaiting here means we would never progress.
    /* eslint-disable @typescript-eslint/no-floating-promises */
    observeCcmReceived(
      sourceAsset,
      destAsset,
      destAddress,
      ccmMetadata,
      undefined,
      () => stopObservingCcmReceived,
    ).then((event) => {
      if (event !== undefined) {
        throw new Error(`${tag} CCM event emitted. Transaction should not have been broadcasted!`);
      }
    });
    // Expect Broadcast Aborted
    testGasLimitCcmSwaps.log(
      `${tag} Gas budget of ${gasLimitBudget} is too low. Expecting BroadcastAborted event.`,
    );
    await observeEvent(`${destChain.toLowerCase()}Broadcaster:BroadcastAborted`, {
      test: (event) => event.data.broadcastId === broadcastId,
    }).event;
    stopObservingCcmReceived = true;
    testGasLimitCcmSwaps.log(`${tag} Broadcast Aborted found! broadcastId: ${broadcastId}`);
  } else if (minGasLimitRequired + evmBaseComputeOverHead < gasLimitBudget) {
    testGasLimitCcmSwaps.log(
      `${tag} Gas budget ${gasLimitBudget}. Expecting successful broadcast.`,
    );

    // Check that broadcast is not aborted
    const observeBroadcastFailure = observeBadEvent(
      `${destChain.toLowerCase()}Broadcaster:BroadcastAborted`,
      {
        test: (event) => {
          const aborted = event.data.broadcastId === broadcastId;
          if (aborted) {
            testGasLimitCcmSwaps.log(
              `${tag} FAILURE! Broadcast Aborted unexpected! broadcastId: ${
                event.data.broadcastId
              }. Gas budget: ${gasLimitBudget} while limit is ${
                minGasLimitRequired + evmBaseComputeOverHead
              }!`,
            );
          }
          return aborted;
        },
        label: testTag,
      },
    );

    testGasLimitCcmSwaps.log(`${tag} Waiting for CCM event...`);

    // Expecting success
    const ccmReceived = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }

    testGasLimitCcmSwaps.log(`${tag} CCM event emitted!`);

    // Stop listening for broadcast failure
    await observeBroadcastFailure.stop();

    const receipt = await web3.eth.getTransactionReceipt(ccmReceived?.txHash as string);
    const tx = await web3.eth.getTransaction(ccmReceived?.txHash as string);
    const gasUsed = receipt.gasUsed;
    const gasPrice = tx.gasPrice;
    const totalFee = gasUsed * Number(gasPrice);

    const feeDeficitHandle = observeEvent(
      `${destChain.toLowerCase()}Broadcaster:TransactionFeeDeficitRecorded`,
      { test: (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee },
    );

    if (tx.maxFeePerGas !== maxFeePerGas.toString()) {
      throw new Error(
        `${tag} Tx Max fee per gas ${tx.maxFeePerGas} different than expected ${maxFeePerGas}`,
      );
    }
    if (tx.gas !== Math.min(gasLimitBudget, getEngineBroadcastLimit(destChain))) {
      throw new Error(`${tag} Tx gas limit ${tx.gas} different than expected ${gasLimitBudget}`);
    }
    // This should not happen by definition, as maxFeePerGas * gasLimit < egressBudgetAmount
    if (totalFee > egressBudgetAmount) {
      throw new Error(
        `${tag} Transaction fee paid is higher than the budget paid by the user! totalFee: ${totalFee} egressBudgetAmount: ${egressBudgetAmount}`,
      );
    }
    testGasLimitCcmSwaps.log(`${tag} Swap success! TxHash: ${ccmReceived?.txHash}!`);

    testGasLimitCcmSwaps.log(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle.event;
    testGasLimitCcmSwaps.log(`${tag} Fee deficit recorded!`);
  } else {
    testGasLimitCcmSwaps.log(`${tag} Budget too tight, can't determine if swap should succeed.`);
  }
}

async function testRandomConsumptionTestEvm(sourceAsset: Asset, destAsset: Asset) {
  function getRandomGasConsumption(chain: Chain): number {
    const range = MAX_TEST_GAS_CONSUMPTION[chain] - MIN_TEST_GAS_CONSUMPTION[chain] + 1;
    let randomInt = Math.floor(Math.random() * range) + MIN_TEST_GAS_CONSUMPTION[chain];
    while (usedNumbers.has(randomInt)) {
      randomInt = Math.floor(Math.random() * range) + MIN_TEST_GAS_CONSUMPTION[chain];
    }
    usedNumbers.add(randomInt);
    return randomInt;
  }

  testGasLimitSwapToEvm(
    sourceAsset,
    destAsset,
    'randGasConsumption',
    getRandomGasConsumption(chainFromAsset(destAsset)),
  );
}

// Spamming to raise Ethereum's fee, otherwise it will get stuck at almost zero fee (~7 wei)
let spam = true;

async function spamChain(chain: Chain) {
  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum':
      spamEvm('Ethereum', 500, () => spam);
      break;
    case 'Solana':
      spamSolana(getChainMinFee('Solana'), 100, () => spam);
      break;
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
}

export async function main() {
  const feeDeficitRefused = observeBadEvent(':TransactionFeeDeficitRefused', {});
  testGasLimitCcmSwaps.log('Spamming chains to increase fees...');

  const spammingEth = spamChain('Ethereum');
  const spammingArb = spamChain('Arbitrum');
  const spammingSol = spamChain('Solana');

  // Wait for the fees to increase to the stable expected amount
  let i = 0;
  const ethMinPriorityFee = getChainMinFee('Ethereum');
  const arbMinBaseFee = getChainMinFee('Arbitrum');
  const solMinPrioFee = getChainMinFee('Solana');
  while (
    (await getChainFees('Ethereum')).priorityFee < ethMinPriorityFee ||
    (await getChainFees('Arbitrum')).baseFee < arbMinBaseFee ||
    (await getChainFees('Solana')).priorityFee < solMinPrioFee
  ) {
    if (++i > LOOP_TIMEOUT) {
      spam = false;
      await spammingEth;
      await spammingArb;
      await spammingSol;
      testGasLimitCcmSwaps.log(
        "Skipping gasLimit CCM test as the priority fee didn't increase enough",
      );
      return;
    }
    await sleep(500);
  }

  const randomConsumptionTestEvm = [
    testRandomConsumptionTestEvm('Dot', 'Flip'),
    testRandomConsumptionTestEvm('Eth', 'Usdc'),
    testRandomConsumptionTestEvm('Eth', 'Usdt'),
    testRandomConsumptionTestEvm('Flip', 'Eth'),
    testRandomConsumptionTestEvm('Btc', 'Eth'),
    testRandomConsumptionTestEvm('Dot', 'ArbEth'),
    testRandomConsumptionTestEvm('Eth', 'ArbUsdc'),
    testRandomConsumptionTestEvm('Flip', 'ArbEth'),
    testRandomConsumptionTestEvm('ArbEth', 'Eth'),
    testRandomConsumptionTestEvm('Sol', 'ArbUsdc'),
    testRandomConsumptionTestEvm('SolUsdc', 'Eth'),
  ];

  // Gas budget to 10% of the default swap amount, which should be enough
  const gasLimitSwapsSufBudget = [
    testGasLimitSwapToEvm('Dot', 'Usdc', 'sufBudget', 10),
    testGasLimitSwapToEvm('Usdc', 'Eth', 'sufBudget', 10),
    testGasLimitSwapToEvm('Flip', 'Usdt', 'sufBudget', 10),
    testGasLimitSwapToEvm('Usdt', 'Eth', 'sufBudget', 10),
    testGasLimitSwapToEvm('Btc', 'Flip', 'sufBudget', 10),
    testGasLimitSwapToEvm('Dot', 'ArbEth', 'sufBudget', 10),
    testGasLimitSwapToEvm('Eth', 'ArbUsdc', 'sufBudget', 10),
    testGasLimitSwapToEvm('ArbEth', 'Flip', 'sufBudget', 10),
    testGasLimitSwapToEvm('Btc', 'ArbUsdc', 'sufBudget', 10),
    testGasLimitSwapToEvm('Eth', 'ArbEth', 'sufBudget', 10),
    testGasLimitSwapToEvm('ArbUsdc', 'Flip', 'sufBudget', 10),
    testGasLimitSwapToEvm('Sol', 'Usdc', 'sufBudget', 10),
    testGasLimitSwapToEvm('SolUsdc', 'ArbEth', 'sufBudget', 10),
    testGasLimitSwapToSolana('Usdc', 'Sol', 'sufBudget', 10),
    testGasLimitSwapToSolana('Btc', 'Sol', 'sufBudget', 100),
    testGasLimitSwapToSolana('Dot', 'Sol', 'sufBudget', 10),
    testGasLimitSwapToSolana('ArbUsdc', 'SolUsdc', 'sufBudget', 10),
    testGasLimitSwapToSolana('Eth', 'SolUsdc', 'sufBudget', 10),
  ];

  // This amount of gasLimitBudget will be swapped into very little gasLimitBudget. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap input amount. So for now we provide the minimum so it gets swapped to just > 0.
  const gasLimitSwapsInsufBudget = [
    testGasLimitSwapToEvm('Dot', 'Flip', 'insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Eth', 'Usdc', 'insufBudget', 10 ** 8),
    testGasLimitSwapToEvm('Eth', 'Usdt', 'insufBudget', 10 ** 8),
    testGasLimitSwapToEvm('Flip', 'Eth', 'insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Btc', 'Eth', 'insufBudget', 10 ** 5),
    testGasLimitSwapToEvm('Dot', 'ArbUsdc', 'insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Eth', 'ArbEth', 'insufBudget', 10 ** 8),
    testGasLimitSwapToEvm('Flip', 'ArbUsdc', 'insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Btc', 'ArbEth', 'insufBudget', 10 ** 5),
    testGasLimitSwapToEvm('ArbEth', 'Eth', 'insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('ArbUsdc', 'Flip', 'insufBudget', 10 ** 5),
    testGasLimitSwapToEvm('Sol', 'Usdc', 'sufBudget', 10 ** 6),
    testGasLimitSwapToEvm('SolUsdc', 'Eth', 'sufBudget', 10 ** 6),
    testGasLimitSwapToSolana('Btc', 'Sol', 'insufBudget', 10 ** 6),
    testGasLimitSwapToSolana('Dot', 'Sol', 'insufBudget', 10 ** 6),
    testGasLimitSwapToSolana('ArbUsdc', 'SolUsdc', 'insufBudget', 10 ** 7),
    testGasLimitSwapToSolana('Eth', 'SolUsdc', 'insufBudget', 10 ** 8),
  ];

  await Promise.all([
    ...gasLimitSwapsSufBudget,
    ...randomConsumptionTestEvm,
    ...gasLimitSwapsInsufBudget,
  ]);

  spam = false;
  await spammingEth;
  await spammingArb;
  await spammingSol;

  // Make sure all the spamming has stopped to avoid triggering connectivity issues when running the next test.
  await sleep(10000);
  await feeDeficitRefused.stop();
}
