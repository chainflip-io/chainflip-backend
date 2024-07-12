import Web3 from 'web3';
import { PublicKey } from '@solana/web3.js';
import { InternalAsset as Asset, Chain } from '@chainflip/cli';
import { newCcmMetadata, prepareSwap } from './swapping';
import {
  ccmSupportedChains,
  chainFromAsset,
  chainGasAsset,
  getEvmEndpoint,
  getSolConnection,
  observeCcmReceived,
  observeSwapScheduled,
  sleep,
  SwapType,
} from './utils';
import { requestNewSwap } from './perform_swap';
import { send } from './send';
import { spamEvm } from './send_evm';
import { observeEvent, observeBadEvent } from './utils/substrate';
import { CcmDepositMetadata } from './new_swap';
import { spamSolana } from './send_sol';

const LOOP_TIMEOUT = 15;
const LAMPORTS_PER_SIGNATURE = 5000;
// Arbitrary default gas consumption values for testing.
const DEFAULT_GAS_CONSUMPTION: Record<string, number> = { Ethereum: 260000, Arbitrum: 3000000 };
// The base overhead increases with message lenght. This is an approximation => BASE_GAS_OVERHEAD + messageLength * gasPerByte
// EVM requires 16 gas per calldata byte so a reasonable approximation is 17 to cover hashing and other operations over the data.
const EVM_GAS_PER_BYTE = 17;

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
  const trackedData = (
    await observeEvent(`${chain.toLowerCase()}ChainTracking:ChainStateUpdated`).event
  ).data.newChainState.trackedData;

  let baseFee = 0;
  if (chain !== 'Solana') {
    baseFee = Number(trackedData.baseFee.replace(/,/g, ''));
  }
  // Arbitrum doesn't have priority fee
  let priorityFee = 0;
  if (chain !== 'Arbitrum') {
    priorityFee = Number(trackedData.priorityFee.replace(/,/g, ''));
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

  if (chainGasAsset(destChain) !== sourceAsset) {
    gasSwapScheduledHandle = observeSwapScheduled(
      sourceAsset,
      chainGasAsset(destChain),
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
    `${destChain.toString().toLowerCase()}IngressEgress:CcmBroadcastRequested`,
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
    `${destChain.toString().toLowerCase()}Broadcaster:TransactionBroadcastRequest`,
    {
      test: (event) => {
        broadcastIdToTxPayload[event.data.broadcastId] = event.data.transactionPayload;
        return false;
      },
      abortable: true,
    },
  );

  await send(sourceAsset, depositAddress);

  const swapId = (await swapScheduledHandle).data.swapId;

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
    swapExecutedHandle.event,
    swapEgressHandle.event,
    ccmBroadcastHandle.event,
    broadcastRequesthandle.event,
  ]);

  let egressBudgetAmount;

  if (chainGasAsset(destChain) === sourceAsset) {
    egressBudgetAmount = messageMetadata.gasBudget;
  } else {
    console.log(`${tag} Waiting for gas swap to be scheduled`);
    const {
      data: { swapId: gasSwapId },
    } = await gasSwapScheduledHandle!;
    egressBudgetAmount = Number(swapIdToEgressAmount[gasSwapId].replace(/,/g, ''));
  }

  console.log(`${tag} Egress budget amount: ${egressBudgetAmount}`);
  const egressId = swapIdToEgressId[swapId];
  const broadcastId = egressIdToBroadcastId[egressId];
  const txPayload = broadcastIdToTxPayload[broadcastId];
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

  console.log(`${tag} Finished tracking events`);

  const { priorityFee: computePrice } = await getChainFees('Solana');

  if (computePrice === 0) {
    throw new Error('Compute price shouldnt be 0');
  }
  const gasLimitBudget = Math.floor(
    Math.max(0, egressBudgetAmount - LAMPORTS_PER_SIGNATURE) / Math.ceil(computePrice / 10 ** 6),
  );

  const minGasLimitRequired = getMinGasOverhead('Solana');
  const solanaBaseComputeOverHead = getBaseGasOverheadBuffer(destChain);
  const connection = getSolConnection();

  if (minGasLimitRequired >= gasLimitBudget + solanaBaseComputeOverHead) {
    console.log(`${tag} Gas too low, transaction expected to revert`);
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
          // This doesn't throw an error, for now it's fine to print it.
          if (transaction?.meta?.err === null) {
            throw new Error('Transaction should have reverted');
          }
          break;
        }
        await sleep(2000);
        attempts++;
      }
    }

    if (transaction === null) {
      throw new Error('Transaction not found');
    }
  } else if (minGasLimitRequired + solanaBaseComputeOverHead < gasLimitBudget) {
    console.log(`${tag} Gas budget ${gasLimitBudget}. Expecting successful broadcast.`);

    const txHash = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
    console.log(`${tag} CCM event emitted!`);

    const transaction = await connection.getTransaction(txHash as string, {
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

    // This should not happen by definition, as maxFeePerGas * gasLimit < egressBudgetAmount
    if (totalFee > egressBudgetAmount) {
      throw new Error(
        `${tag} Transaction fee paid is higher than the budget paid by the user! totalFee: ${totalFee} egressBudgetAmount: ${egressBudgetAmount}`,
      );
    }
    console.log(`${tag} CCM Swap success! TxHash: ${txHash}!`);
    console.log(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle.event;
    console.log(`${tag} Fee deficit recorded!`);
  } else {
    console.log(`${tag} Budget too tight, can't determine if swap should succeed.`);
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

  const maxFeePerGas = Number(txPayload.maxFeePerGas.replace(/,/g, ''));
  const gasLimitBudget = Number(txPayload.gasLimit.replace(/,/g, ''));
  const byteLength = Web3.utils.hexToBytes(ccmMetadata.message).length;
  const minGasLimitRequired =
    gasConsumption + getMinGasOverhead(destChain) + byteLength * EVM_GAS_PER_BYTE;

  const evmBaseComputeOverHead = getBaseGasOverheadBuffer(destChain);
  // This is a very rough approximation for the gas limit required. A buffer is added to account for that.

  if (minGasLimitRequired >= gasLimitBudget + evmBaseComputeOverHead) {
    let stopObservingCcmReceived = false;

    console.log(
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
    console.log(
      `${tag} Gas budget of ${gasLimitBudget} is too low. Expecting BroadcastAborted event.`,
    );
    await observeEvent(`${destChain.toLowerCase()}Broadcaster:BroadcastAborted`, {
      test: (event) => event.data.broadcastId === broadcastId,
    }).event;
    stopObservingCcmReceived = true;
    console.log(`${tag} Broadcast Aborted found! broadcastId: ${broadcastId}`);
  } else if (minGasLimitRequired + evmBaseComputeOverHead < gasLimitBudget) {
    console.log(`${tag} Gas budget ${gasLimitBudget}. Expecting successful broadcast.`);

    // Check that broadcast is not aborted
    const observeBroadcastFailure = observeBadEvent(
      `${destChain.toLowerCase()}Broadcaster:BroadcastAborted`,
      {
        test: (event) => {
          const aborted = event.data.broadcastId === broadcastId;
          if (aborted) {
            console.log(
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

    console.log(`${tag} Waiting for CCM event...`);

    // Expecting success
    const ccmReceived = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }

    console.log(`${tag} CCM event emitted!`);

    // Stop listening for broadcast failure
    await observeBroadcastFailure.stop();
    console.log(`${tag} Stopped listening to broadcast failure!`);

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
    console.log(
      `${tag} Swap success! TxHash: ${typeof ccmReceived === 'string' ? ccmReceived : (ccmReceived?.txHash as string)}!`,
    );

    console.log(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle.event;
    console.log(`${tag} Fee deficit recorded!`);
  } else {
    console.log(`${tag} Budget too tight, can't determine if swap should succeed.`);
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
    ' randGasConsumption',
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

export async function testGasLimitCcmSwaps() {
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
      console.log("=== Skipping gasLimit CCM test as the priority fee didn't increase enough. ===");
      return;
    }
    await sleep(500);
  }
  console.log('Success!! Fess reached the minimum amount!');
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
    testGasLimitSwapToEvm('Dot', 'Usdc', ' sufBudget', 10),
    testGasLimitSwapToEvm('Usdc', 'Eth', ' sufBudget', 10),
    testGasLimitSwapToEvm('Flip', 'Usdt', ' sufBudget', 10),
    testGasLimitSwapToEvm('Usdt', 'Eth', ' sufBudget', 10),
    testGasLimitSwapToEvm('Btc', 'Flip', ' sufBudget', 10),
    testGasLimitSwapToEvm('Dot', 'ArbEth', ' sufBudget', 10),
    testGasLimitSwapToEvm('Eth', 'ArbUsdc', ' sufBudget', 10),
    testGasLimitSwapToEvm('ArbEth', 'Flip', ' sufBudget', 10),
    testGasLimitSwapToEvm('Btc', 'ArbUsdc', ' sufBudget', 10),
    testGasLimitSwapToEvm('Eth', 'ArbEth', ' sufBudget', 10),
    testGasLimitSwapToEvm('ArbUsdc', 'Flip', ' sufBudget', 10),

    testGasLimitSwapToEvm('Sol', 'Usdc', ' sufBudget', 10),
    testGasLimitSwapToEvm('SolUsdc', 'ArbEth', ' sufBudget', 10),
    testGasLimitSwapToSolana('Btc', 'Sol', ' sufBudget', 10),
    testGasLimitSwapToSolana('Dot', 'Sol', ' sufBudget', 10),
    testGasLimitSwapToSolana('ArbUsdc', 'SolUsdc', ' sufBudget', 10),
    testGasLimitSwapToSolana('Eth', 'SolUsdc', ' sufBudget', 10),
  ];

  // This amount of gasLimitBudget will be swapped into very little gasLimitBudget. Not into zero as that will cause a debug_assert to
  // panic when not in release due to zero swap input amount. So for now we provide the minimum so it gets swapped to just > 0.
  const gasLimitSwapsInsufBudget = [
    testGasLimitSwapToEvm('Dot', 'Flip', ' insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Eth', 'Usdc', ' insufBudget', 10 ** 8),
    testGasLimitSwapToEvm('Eth', 'Usdt', ' insufBudget', 10 ** 8),
    testGasLimitSwapToEvm('Flip', 'Eth', ' insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Btc', 'Eth', ' insufBudget', 10 ** 5),
    testGasLimitSwapToEvm('Dot', 'ArbUsdc', ' insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Eth', 'ArbEth', ' insufBudget', 10 ** 8),
    testGasLimitSwapToEvm('Flip', 'ArbUsdc', ' insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('Btc', 'ArbEth', ' insufBudget', 10 ** 5),
    testGasLimitSwapToEvm('ArbEth', 'Eth', ' insufBudget', 10 ** 6),
    testGasLimitSwapToEvm('ArbUsdc', 'Flip', ' insufBudget', 10 ** 5),

    testGasLimitSwapToSolana('Btc', 'Sol', ' insufBudget', 10 ** 6),
    testGasLimitSwapToSolana('Dot', 'Sol', ' insufBudget', 10 ** 6),
    testGasLimitSwapToSolana('ArbUsdc', 'SolUsdc', ' insufBudget', 10 ** 7),
    testGasLimitSwapToSolana('Eth', 'SolUsdc', ' insufBudget', 10 ** 8),
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
}
