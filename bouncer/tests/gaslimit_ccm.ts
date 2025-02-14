import Web3 from 'web3';
import { InternalAsset as Asset, Chain } from '@chainflip/cli';
import { newCcmMetadata, prepareSwap } from '../shared/swapping';
import {
  ccmSupportedChains,
  chainFromAsset,
  EgressId,
  getEvmEndpoint,
  getSolConnection,
  observeCcmReceived,
  observeSwapRequested,
  sleep,
  SwapRequestType,
  TransactionOrigin,
} from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { send } from '../shared/send';
import { estimateCcmCfTesterGas, spamEvm } from '../shared/send_evm';
import { observeEvent, observeBadEvent } from '../shared/utils/substrate';
import { CcmDepositMetadata } from '../shared/new_swap';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

// Minimum and maximum gas consumption values to be in a useful range for testing. Not using very low numbers
// to avoid flakiness in the tests expecting a broadcast abort due to not having enough gas.
const RANGE_TEST_GAS_CONSUMPTION: Record<string, { min: number; max: number }> = {
  Ethereum: { min: 150000, max: 1000000 },
  Arbitrum: { min: 3000000, max: 5000000 },
};

const LOOP_TIMEOUT = 15;

// After the swap is complete, we search for the expected swap event in this many past blocks.
const CHECK_PAST_BLOCKS_FOR_EVENTS = 30;

function getEngineBroadcastLimit(chain: Chain): number {
  switch (chain) {
    case 'Ethereum':
      return 10000000;
    case 'Arbitrum':
      return 25000000;
    case 'Solana':
      return 600000;
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
    default:
      throw new Error(`Chain ${chain} is not expected to have a minimum fee`);
  }
}

async function getChainFees(logger: Logger, chain: Chain) {
  let baseFee = 0;
  let priorityFee = 0;

  const trackedData = (
    await observeEvent(logger, `${chain.toLowerCase()}ChainTracking:ChainStateUpdated`).event
  ).data.newChainState.trackedData;

  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum': {
      baseFee = Number(trackedData.baseFee.replace(/,/g, ''));

      if (chain === 'Ethereum') {
        priorityFee = Number(trackedData.priorityFee.replace(/,/g, ''));
      }
      break;
    }
    case 'Solana': {
      priorityFee = Number(trackedData.priorityFee.replace(/,/g, ''));
      break;
    }
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
  return { baseFee, priorityFee };
}

async function executeAndTrackCcmSwap(
  logger: Logger,
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
    logger,
    sourceAsset,
    destAsset,
    undefined,
    messageMetadata,
    `GasLimit${testTag || ''}`,
  );

  const { depositAddress, channelId } = await requestNewSwap(
    logger,
    sourceAsset,
    destAsset,
    destAddress,
    tag,
    messageMetadata,
  );
  const swapRequestedHandle = observeSwapRequested(
    logger,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId },
    SwapRequestType.Regular,
  );
  await send(logger, sourceAsset, depositAddress);
  const swapRequestId = Number((await swapRequestedHandle).data.swapRequestId.replaceAll(',', ''));

  // Find all of the swap events
  const egressId = (
    await observeEvent(logger, 'swapping:SwapEgressScheduled', {
      test: (event) => Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.egressId as EgressId;
  logger.debug(`${tag} Found egressId: ${egressId}`);

  const broadcastId = (
    await observeEvent(logger, `${destChain.toLowerCase()}IngressEgress:CcmBroadcastRequested`, {
      test: (event) =>
        event.data.egressId[0] === egressId[0] && event.data.egressId[1] === egressId[1],
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.broadcastId;
  logger.debug(`${tag} Found broadcastId: ${broadcastId}`);

  const txPayload = (
    await observeEvent(
      logger,
      `${destChain.toLowerCase()}Broadcaster:TransactionBroadcastRequest`,
      {
        test: (event) => event.data.broadcastId === broadcastId,
        historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
      },
    ).event
  ).data.transactionPayload;
  logger.debug(`${tag} Found txPayload: ${txPayload}`);

  return { tag, destAddress, broadcastId, txPayload };
}

async function testGasLimitSwapToSolana(logger: Logger, sourceAsset: Asset, destAsset: Asset) {
  const connection = getSolConnection();
  const destChain = chainFromAsset(destAsset);

  if (destChain !== 'Solana') {
    throw new Error(`Destination chain ${destChain} is not Solana`);
  }

  const ccmMetadata = await newCcmMetadata(destAsset);

  const { tag, destAddress } = await executeAndTrackCcmSwap(
    logger,
    sourceAsset,
    destAsset,
    ccmMetadata,
  );

  logger.debug(`${tag} Finished tracking events`);

  const { priorityFee: computePrice } = await getChainFees(logger, 'Solana');

  if (computePrice === 0) {
    throw new Error('Compute price should not be 0');
  }

  logger.debug(`${tag} Expecting successful CCM broadcast.`);

  const ccmEvent = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
  const txSignature = ccmEvent?.txHash as string;
  logger.debug(`${tag} CCM event emitted!`);

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
    logger,
    `${destChain.toLowerCase()}Broadcaster:TransactionFeeDeficitRecorded`,
    { test: (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee },
  );
  logger.debug(`${tag} CCM Swap success! TxHash: ${txSignature}!`);
  logger.debug(`${tag} Waiting for a fee deficit to be recorded...`);
  await feeDeficitHandle.event;
  logger.debug(`${tag} Fee deficit recorded!`);
}

// Using unique gas consumption amount since the CCM message is used as unique identifier
// when observing the CCM event on the destination chain.
const usedGasConsumptionAmount = new Set<number>();

async function testGasLimitSwapToEvm(
  parentLogger: Logger,
  sourceAsset: Asset,
  destAsset: Asset,
  abortTest: boolean = false,
) {
  function getRandomGasConsumption(chain: string): number {
    const { min, max } = RANGE_TEST_GAS_CONSUMPTION[chain];
    const range = max - min + 1;
    let randomInt = Math.floor(Math.random() * range) + min;
    while (usedGasConsumptionAmount.has(randomInt)) {
      randomInt = Math.floor(Math.random() * range) + min;
    }
    usedGasConsumptionAmount.add(randomInt);
    return randomInt;
  }

  const destChain = chainFromAsset(destAsset);
  const web3 = new Web3(getEvmEndpoint(chainFromAsset(destAsset)));

  if (destChain !== 'Arbitrum' && destChain !== 'Ethereum') {
    throw new Error(`Destination chain ${destChain} is not Ethereum nor Arbitrum`);
  }

  const gasConsumption = getRandomGasConsumption(chainFromAsset(destAsset));

  const ccmMetadata = await newCcmMetadata(
    destAsset,
    web3.eth.abi.encodeParameters(['string', 'uint256'], ['GasTest', gasConsumption]),
  );

  // Estimating gas separately. We can't rely on the default gas estimation in `newCcmMetadata()`
  // because the CF tester gas consumption depends on the gas limit, making this a circular calculation.
  // Instead, we get a base calculation with an empty message that doesn't run the gas consumption.
  const baseCfTesterGas = await estimateCcmCfTesterGas(destChain, '0x');

  // Adding buffers on both ends to avoid flakiness.
  if (abortTest) {
    // Chainflip overestimates the overhead for safety so we use a 25% buffer to ensure that
    // the gas budget is too low.We also apply a 50% on the baseCfTesterGas since it's highly unreliable.
    ccmMetadata.gasBudget = Math.round(gasConsumption * 0.75 + baseCfTesterGas * 0.5).toString();
  } else {
    // A small buffer should work (10%) as CF should be overestimate, not underestimate
    ccmMetadata.gasBudget = (baseCfTesterGas + Math.round(gasConsumption * 1.1)).toString();
  }

  const testTag = abortTest ? `InsufficientGas` : '';

  const { tag, destAddress, broadcastId, txPayload } = await executeAndTrackCcmSwap(
    parentLogger,
    sourceAsset,
    destAsset,
    ccmMetadata,
    testTag,
  );
  const logger = parentLogger.child({ tag });
  logger.debug(`${tag} Finished tracking events`);

  const maxFeePerGas = Number(txPayload.maxFeePerGas.replace(/,/g, ''));
  const gasLimitBudget = Number(txPayload.gasLimit.replace(/,/g, ''));

  logger.debug(
    `Expecting broadcast ${abortTest ? 'abort' : 'success'}. Broadcast gas budget: ${gasLimitBudget}, user gasBudget ${ccmMetadata.gasBudget} cfTester gasConsumption ${gasConsumption}`,
  );

  if (abortTest) {
    // Expect Broadcast Aborted
    let stopObservingCcmReceived = false;

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
        throw new Error(`$CCM event emitted. Transaction should not have been broadcasted!`);
      }
    });
    await observeEvent(logger, `${destChain.toLowerCase()}Broadcaster:BroadcastAborted`, {
      test: (event) => event.data.broadcastId === broadcastId,
    }).event;
    stopObservingCcmReceived = true;
    logger.debug(`Broadcast Aborted found! broadcastId: ${broadcastId}`);
  } else {
    // Check that broadcast is not aborted
    const observeBroadcastFailure = observeBadEvent(
      logger,
      `${destChain.toLowerCase()}Broadcaster:BroadcastAborted`,
      {
        test: (event) => {
          const aborted = event.data.broadcastId === broadcastId;
          if (aborted) {
            throw new Error(
              `FAILURE! Broadcast Aborted unexpected! broadcastId: ${event.data.broadcastId}. Gas budget: ${gasLimitBudget}`,
            );
          }
          return aborted;
        },
      },
    );

    logger.debug(`Waiting for CCM event...`);

    // Expecting success
    const ccmReceived = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }

    logger.debug(`$CCM event emitted!`);

    // Stop listening for broadcast failure
    await observeBroadcastFailure.stop();

    const receipt = await web3.eth.getTransactionReceipt(ccmReceived?.txHash as string);
    const tx = await web3.eth.getTransaction(ccmReceived?.txHash as string);
    const gasUsed = receipt.gasUsed;
    const gasPrice = tx.gasPrice;
    const totalFee = gasUsed * Number(gasPrice);

    const feeDeficitHandle = observeEvent(
      logger,
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

    logger.debug(`${tag} Swap success! TxHash: ${ccmReceived?.txHash}!`);

    logger.debug(`${tag} Waiting for a fee deficit to be recorded...`);
    await feeDeficitHandle.event;
    logger.debug(`${tag} Fee deficit recorded!`);
  }
}

async function testEvmInsufficientGas(logger: Logger, sourceAsset: Asset, destAsset: Asset) {
  await testGasLimitSwapToEvm(logger, sourceAsset, destAsset, true);
}

// Spamming to raise Ethereum's fee, otherwise it will get stuck at almost zero fee (~7 wei)
let spam = true;

async function spamChain(logger: Logger, chain: Chain) {
  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum':
      spamEvm(logger, 'Ethereum', 500, () => spam);
      break;
    default:
      throw new Error(`Chain ${chain} is not supported for CCM`);
  }
}

export async function testGasLimitCcmSwaps(testContext: TestContext) {
  const logger = testContext.logger;
  const feeDeficitRefused = observeBadEvent(logger, ':TransactionFeeDeficitRefused', {});
  logger.debug('Spamming chains to increase fees...');

  // No need to spam Solana since we are hardcoding the priority fees on the SC
  // and the chain "base fee" don't increase anyway..
  const spammingEth = spamChain(logger, 'Ethereum');
  const spammingArb = spamChain(logger, 'Arbitrum');

  // Wait for the fees to increase to the stable expected amount
  let i = 0;
  const ethMinPriorityFee = getChainMinFee('Ethereum');
  const arbMinBaseFee = getChainMinFee('Arbitrum');
  while (
    (await getChainFees(logger, 'Ethereum')).priorityFee < ethMinPriorityFee ||
    (await getChainFees(logger, 'Arbitrum')).baseFee < arbMinBaseFee
  ) {
    if (++i > LOOP_TIMEOUT) {
      spam = false;
      await spammingEth;
      await spammingArb;
      throw new Error(`Chain fees did not increase enough for the CCM gas limit test to run`);
    }
    await sleep(500);
  }

  const insufficientGasTestEvm = [
    testEvmInsufficientGas(logger, 'Dot', 'Flip'),
    testEvmInsufficientGas(logger, 'Eth', 'Usdc'),
    testEvmInsufficientGas(logger, 'Eth', 'Usdt'),
    testEvmInsufficientGas(logger, 'Flip', 'Eth'),
    testEvmInsufficientGas(logger, 'Btc', 'Eth'),
    testEvmInsufficientGas(logger, 'Dot', 'ArbEth'),
    testEvmInsufficientGas(logger, 'Eth', 'ArbUsdc'),
    testEvmInsufficientGas(logger, 'Flip', 'ArbEth'),
    testEvmInsufficientGas(logger, 'ArbEth', 'Eth'),
    testEvmInsufficientGas(logger, 'Sol', 'ArbUsdc'),
    testEvmInsufficientGas(logger, 'SolUsdc', 'Eth'),
  ];

  const gasLimitSwapsSufBudget = [
    testGasLimitSwapToEvm(logger, 'Dot', 'Usdc'),
    testGasLimitSwapToEvm(logger, 'Usdc', 'Eth'),
    testGasLimitSwapToEvm(logger, 'Flip', 'Usdt'),
    testGasLimitSwapToEvm(logger, 'Usdt', 'Eth'),
    testGasLimitSwapToEvm(logger, 'Btc', 'Flip'),
    testGasLimitSwapToEvm(logger, 'Dot', 'ArbEth'),
    testGasLimitSwapToEvm(logger, 'Eth', 'ArbUsdc'),
    testGasLimitSwapToEvm(logger, 'ArbEth', 'Flip'),
    testGasLimitSwapToEvm(logger, 'Btc', 'ArbUsdc'),
    testGasLimitSwapToEvm(logger, 'Eth', 'ArbEth'),
    testGasLimitSwapToEvm(logger, 'ArbUsdc', 'Flip'),
    testGasLimitSwapToEvm(logger, 'Sol', 'Usdc'),
    testGasLimitSwapToEvm(logger, 'SolUsdc', 'ArbEth'),
    testGasLimitSwapToSolana(logger, 'Usdc', 'Sol'),
    testGasLimitSwapToSolana(logger, 'Btc', 'Sol'),
    testGasLimitSwapToSolana(logger, 'Dot', 'Sol'),
    testGasLimitSwapToSolana(logger, 'ArbUsdc', 'SolUsdc'),
    testGasLimitSwapToSolana(logger, 'Eth', 'SolUsdc'),
  ];

  await Promise.all([...gasLimitSwapsSufBudget, ...insufficientGasTestEvm]);

  spam = false;
  await spammingEth;
  await spammingArb;

  // Make sure all the spamming has stopped to avoid triggering connectivity issues when running the next test.
  await sleep(10000);
  await feeDeficitRefused.stop();
}
