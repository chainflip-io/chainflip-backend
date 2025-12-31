import Web3 from 'web3';
import { InternalAsset as Asset, Chain } from '@chainflip/cli';
import { newCcmMetadata, prepareSwap } from 'shared/swapping';
import {
  ccmSupportedChains,
  chainFromAsset,
  EgressId,
  getEvmEndpoint,
  getEvmWhaleKeypair,
  getSolConnection,
  observeCcmReceived,
  observeSwapRequested,
  sleep,
  SwapRequestType,
  TransactionOrigin,
} from 'shared/utils';
import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { estimateCcmCfTesterGas, signAndSendTxEvm } from 'shared/send_evm';
import { observeEvent, observeBadEvent } from 'shared/utils/substrate';
import { CcmDepositMetadata } from 'shared/new_swap';
import { globalLogger, Logger } from 'shared/utils/logger';
import { afterAll, beforeAll, describe } from 'vitest';
import { concurrentTest } from 'shared/utils/vitest';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

// Minimum and maximum gas consumption values to be in a useful range for testing. Not using very low numbers
// to avoid flakiness in the tests expecting a broadcast abort due to not having enough gas.
const RANGE_TEST_GAS_CONSUMPTION: Record<string, { min: number; max: number }> = {
  Ethereum: { min: 150000, max: 1000000 },
  Arbitrum: { min: 3000000, max: 5000000 },
};

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

async function getChainFees(
  logger: Logger,
  chain: Chain,
): Promise<{ baseFee: number; priorityFee: number }> {
  // Only supported for Ethereum, Arbitrum and Solana
  if (!['Ethereum', 'Arbitrum', 'Solana'].includes(chain)) {
    throw new Error(`${chain} does not support CCM`);
  }

  const eventData = (
    await observeEvent(logger, `${chain.toLowerCase()}ChainTracking:ChainStateUpdated`).event
  ).data;

  logger.debug(`${chain} fees: ${JSON.stringify(eventData)}`);

  const { baseFee, priorityFee } = eventData.newChainState.trackedData as {
    baseFee: number | undefined;
    priorityFee: number;
  };

  return { baseFee: baseFee || 0, priorityFee };
}

async function executeAndTrackCcmSwap<A = []>(
  parentCf: ChainflipIO<A>,
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
    parentCf.logger,
    sourceAsset,
    destAsset,
    undefined,
    messageMetadata,
    `GasLimit${testTag || ''}`,
  );
  const cf = parentCf.withChildLogger(tag);

  const { depositAddress, channelId } = await requestNewSwap(
    cf,
    sourceAsset,
    destAsset,
    destAddress,
    messageMetadata,
  );
  const swapRequestedHandle = observeSwapRequested(
    cf.logger,
    sourceAsset,
    destAsset,
    { type: TransactionOrigin.DepositChannel, channelId },
    SwapRequestType.Regular,
  );
  await send(cf.logger, sourceAsset, depositAddress);
  const swapRequestId = (await swapRequestedHandle).data.swapRequestId;

  // Find all of the swap events
  const egressId = (
    await observeEvent(cf.logger, 'swapping:SwapEgressScheduled', {
      test: (event) => event.data.swapRequestId === swapRequestId,
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.egressId as EgressId;
  cf.debug(`${tag} Found egressId: ${egressId}`);

  const broadcastId = (
    await observeEvent(cf.logger, `${destChain.toLowerCase()}IngressEgress:CcmBroadcastRequested`, {
      test: (event) =>
        event.data.egressId[0] === egressId[0] && event.data.egressId[1] === egressId[1],
      historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
    }).event
  ).data.broadcastId;
  cf.debug(`${tag} Found broadcastId: ${broadcastId}`);

  const txPayload = (
    await observeEvent(
      cf.logger,
      `${destChain.toLowerCase()}Broadcaster:TransactionBroadcastRequest`,
      {
        test: (event) => event.data.broadcastId === broadcastId,
        historicalCheckBlocks: CHECK_PAST_BLOCKS_FOR_EVENTS,
      },
    ).event
  ).data.transactionPayload;
  cf.debug(`${tag} Found txPayload: ${txPayload}`);

  return { tag, destAddress, broadcastId, txPayload };
}

async function testGasLimitSwapToSolana<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  const connection = getSolConnection();
  const destChain = chainFromAsset(destAsset);

  if (destChain !== 'Solana') {
    throw new Error(`Destination chain ${destChain} is not Solana`);
  }

  const ccmMetadata = await newCcmMetadata(destAsset);

  const { tag, destAddress } = await executeAndTrackCcmSwap(
    cf,
    sourceAsset,
    destAsset,
    ccmMetadata,
  );

  cf.debug(`${tag} Finished tracking events`);

  const { priorityFee: computePrice } = await getChainFees(cf.logger, 'Solana');

  if (computePrice === 0) {
    throw new Error('Compute price should not be 0');
  }

  cf.debug(`${tag} Expecting successful CCM broadcast.`);

  const ccmEvent = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
  const txSignature = ccmEvent?.txHash as string;
  cf.debug(`${tag} CCM event emitted!`);

  const transaction = await connection.getTransaction(txSignature, {
    commitment: 'confirmed',
    maxSupportedTransactionVersion: 0,
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
    cf.logger,
    `${destChain.toLowerCase()}Broadcaster:TransactionFeeDeficitRecorded`,
    { test: (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee },
  );
  cf.debug(`${tag} CCM Swap success! TxHash: ${txSignature}!`);
  cf.debug(`${tag} Waiting for a fee deficit of ${totalFee} to be recorded...`);
  await feeDeficitHandle.event;
  cf.debug(`${tag} Fee deficit recorded!`);
}

// Using unique gas consumption amount since the CCM message is used as unique identifier
// when observing the CCM event on the destination chain.
const usedGasConsumptionAmount = new Set<number>();

async function testGasLimitSwapToEvm<A = []>(
  cf: ChainflipIO<A>,
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
    cf,
    sourceAsset,
    destAsset,
    ccmMetadata,
    testTag,
  );
  cf.debug(`${tag} Finished tracking events`);

  const maxFeePerGas = Number(txPayload.maxFeePerGas.replace(/,/g, ''));
  const gasLimitBudget = Number(txPayload.gasLimit.replace(/,/g, ''));

  cf.debug(
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
    await observeEvent(cf.logger, `${destChain.toLowerCase()}Broadcaster:BroadcastAborted`, {
      test: (event) => event.data.broadcastId === broadcastId,
    }).event;
    stopObservingCcmReceived = true;
    cf.debug(`Broadcast Aborted found! broadcastId: ${broadcastId}`);
  } else {
    // Check that broadcast is not aborted
    const observeBroadcastFailure = observeBadEvent(
      cf.logger,
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

    cf.debug(`Waiting for CCM event...`);

    // Expecting success
    const ccmReceived = await observeCcmReceived(sourceAsset, destAsset, destAddress, ccmMetadata);
    if (ccmReceived?.returnValues.ccmTestGasUsed < gasConsumption) {
      throw new Error(`${tag} CCM event emitted. Gas consumed is less than expected!`);
    }

    cf.debug(`CCM event emitted!`);

    // Stop listening for broadcast failure
    await observeBroadcastFailure.stop();

    const receipt = await web3.eth.getTransactionReceipt(ccmReceived?.txHash as string);
    const tx = await web3.eth.getTransaction(ccmReceived?.txHash as string);
    const gasUsed = receipt.gasUsed as unknown as number;
    const gasPrice = tx.gasPrice as unknown as number;
    const totalFee = gasUsed * gasPrice;

    const feeDeficitHandle = observeEvent(
      cf.logger,
      `${destChain.toLowerCase()}Broadcaster:TransactionFeeDeficitRecorded`,
      {
        test: (event) => Number(event.data.amount.replace(/,/g, '')) === totalFee,
        historicalCheckBlocks: 10,
      },
    );

    if (tx.maxFeePerGas !== maxFeePerGas.toString()) {
      throw new Error(
        `${tag} Tx Max fee per gas ${tx.maxFeePerGas} different than expected ${maxFeePerGas}`,
      );
    }
    if (tx.gas !== Math.min(gasLimitBudget, getEngineBroadcastLimit(destChain))) {
      throw new Error(`${tag} Tx gas limit ${tx.gas} different than expected ${gasLimitBudget}`);
    }

    cf.debug(`${tag} Swap success! TxHash: ${ccmReceived?.txHash}!`);

    cf.debug(`${tag} Waiting for a fee deficit of ${totalFee} to be recorded...`);
    await feeDeficitHandle.event;
    cf.debug(`${tag} Fee deficit recorded!`);
  }
}

async function testEvmInsufficientGas<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
) {
  await testGasLimitSwapToEvm(cf, sourceAsset, destAsset, true);
}

function spamEvmChain<A = []>(cf: ChainflipIO<A>, chain: Chain): () => void {
  const { pubkey: whalePubkey } = getEvmWhaleKeypair('Ethereum');

  let stop = false;
  const cancel = () => {
    stop = true;
  };

  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum':
      (async () => {
        while (!stop) {
          await signAndSendTxEvm(cf.logger, chain, whalePubkey, '1', undefined, undefined);
          await sleep(200);
        }
      })();

      return cancel;
    default:
      throw new Error(`Chain ${chain} is not an EVM chain`);
  }
}

let stopSpammingEth: () => void;
let stopSpammingArb: () => void;
let feeDeficitRefused: { stop: () => Promise<void> };

describe('GasLimitCcmSwaps', async () => {
  const cf = await newChainflipIO(globalLogger.child({ test: 'GasLimitCcmSwaps' }), []);
  beforeAll(
    async () => {
      feeDeficitRefused = observeBadEvent(cf.logger, ':TransactionFeeDeficitRefused', {});
      cf.info('Spamming chains to increase fees...');

      // No need to spam Solana since we are hardcoding the priority fees on the SC
      // and the chain "base fee" don't increase anyway..
      stopSpammingEth = spamEvmChain(cf, 'Ethereum');
      stopSpammingArb = spamEvmChain(cf, 'Arbitrum');

      // Wait for the fees to increase to the stable expected amount
      const ethMinPriorityFee = getChainMinFee('Ethereum');
      const arbMinBaseFee = getChainMinFee('Arbitrum');

      // eslint-disable-next-line no-constant-condition
      while (true) {
        const [ethFees, arbFees] = await Promise.all([
          getChainFees(cf.logger, 'Ethereum'),
          getChainFees(cf.logger, 'Arbitrum'),
        ]);

        if (ethFees.priorityFee < ethMinPriorityFee || arbFees.baseFee < arbMinBaseFee) {
          cf.debug(
            `Waiting for chain fees to increase. Ethereum priorityFee: ${ethFees.priorityFee} (waiting for ${ethMinPriorityFee}), Arbitrum baseFee: ${arbFees.baseFee} (waiting for ${arbMinBaseFee})`,
          );
        } else {
          cf.info(
            `Spamming successful. Ethereum priorityFee: ${ethFees.priorityFee}, Arbitrum baseFee: ${arbFees.baseFee}`,
          );
          break;
        }

        await sleep(6_000);
      }
    },
    // ETH fees can take a few blocks to increase.
    120_000,
  );

  for (const pair of [
    ['Btc', 'Flip'],
    ['Eth', 'Usdc'],
    ['Eth', 'Usdt'],
    ['Flip', 'Eth'],
    ['Btc', 'Eth'],
    ['Sol', 'ArbEth'],
    ['Eth', 'ArbUsdc'],
    ['Flip', 'ArbEth'],
    ['ArbEth', 'Eth'],
    ['Sol', 'ArbUsdc'],
    ['SolUsdc', 'Eth'],
  ]) {
    concurrentTest(
      `EVM Insufficient Gas CCM swap ${pair[0]} to ${pair[1]}`,
      async (ctx) => {
        // TODO: we're somehow recreating a ChainflipIO from a context, instead of that we should
        // consider redoing the whole pipeline with ChainflipIO
        await testEvmInsufficientGas(
          await newChainflipIO(ctx.logger, []),
          pair[0] as Asset,
          pair[1] as Asset,
        );
      },
      300,
      true,
    );
  }

  for (const pair of [
    ['Sol', 'Usdc'],
    ['Usdc', 'Eth'],
    ['Flip', 'Usdt'],
    ['Usdt', 'Eth'],
    ['Btc', 'Flip'],
    ['Sol', 'ArbEth'],
    ['Eth', 'ArbUsdc'],
    ['ArbEth', 'Flip'],
    ['Btc', 'ArbUsdc'],
    ['Eth', 'ArbEth'],
    ['ArbUsdc', 'Flip'],
    ['Sol', 'Usdc'],
    ['SolUsdc', 'ArbEth'],
  ]) {
    concurrentTest(
      `EVM CCM Gas Limit swap ${pair[0]} to ${pair[1]}`,
      async (ctx) => {
        await testGasLimitSwapToEvm(
          await newChainflipIO(ctx.logger, []),
          pair[0] as Asset,
          pair[1] as Asset,
        );
      },
      300,
      true,
    );
  }

  for (const pair of [
    ['Usdc', 'Sol'],
    ['Btc', 'Sol'],
    ['Flip', 'Sol'],
    ['ArbUsdc', 'SolUsdc'],
    ['Eth', 'SolUsdc'],
  ]) {
    concurrentTest(
      `Solana CCM Gas Limit swap ${pair[0]} to ${pair[1]}`,
      async (ctx) => {
        await testGasLimitSwapToSolana(
          await newChainflipIO(ctx.logger, []),
          pair[0] as Asset,
          pair[1] as Asset,
        );
      },
      300,
      true,
    );
  }

  afterAll(async () => {
    stopSpammingEth();
    stopSpammingArb();

    // Make sure all the spamming has stopped to avoid triggering connectivity issues when running the next test.
    await sleep(10000);
    await feeDeficitRefused.stop();
  }, 20_000);
});
