import axios from 'axios';
import { randomBytes } from 'crypto';
import { InternalAsset } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { sendBtc } from '../shared/send_btc';
import {
  newAddress,
  sleep,
  handleSubstrateError,
  brokerMutex,
  chainGasAsset,
  hexStringToBytesArray,
  observeBroadcastSuccess,
  observeSwapEvents,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';
import { getBalance } from '../shared/get_balance';
import { send } from '../shared/send';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 300);

/**
 * Observes the balance of a BTC address and returns true if the balance changes. Times out after 100 seconds and returns false if the balance does not change.
 *
 * @param address - The address to observe the balance of.
 * @returns - Whether the balance changed.
 */
async function observeBtcAddressBalanceChange(address: string): Promise<boolean> {
  const MAX_RETRIES = 100;
  const initialBalance = await getBtcBalance(address);
  for (let i = 0; i < MAX_RETRIES; i++) {
    await sleep(1000);
    const balance = await getBtcBalance(address);
    if (balance !== initialBalance) {
      return Promise.resolve(true);
    }
  }
  console.error(`BTC balance for ${address} did not change after ${MAX_RETRIES} seconds.`);
  return Promise.resolve(false);
}

/**
 * Generates a new address for an asset.
 *
 * @param asset - The asset to generate an address for.
 * @param seed - The seed to generate the address with. If no seed is provided, a random one is generated.
 * @returns - The new address.
 */
async function newAssetAddress(asset: InternalAsset, seed = null): Promise<string> {
  return Promise.resolve(newAddress(asset, seed || randomBytes(32).toString('hex')));
}

/**
 * Mark a transaction for rejection.
 *
 * @param txId - The txId to submit, in its typical representation in bitcoin explorers,
 * i.e., reverse of its memory representation.
 */
async function markTxForRejection(txId: string, chain: string) {
  // The engine uses the memory representation everywhere, so we convert the txId here.
  const memoryRepresentationTxId = hexStringToBytesArray(txId).reverse();
  await using chainflip = await getChainflipApi();
  switch (chain) {
    case 'Bitcoin':
      return brokerMutex.runExclusive(async () =>
        chainflip.tx.bitcoinIngressEgress
          .markTransactionForRejection(memoryRepresentationTxId)
          .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
      );
    case 'Ethereum':
      return brokerMutex.runExclusive(async () =>
        chainflip.tx.ethereumIngressEgress
          .markTransactionForRejection(txId)
          .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
      );
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
}

/**
 * Submit a post request to the deposit-monitor, with error handling.
 * @param portAndRoute Where we want to submit the request to.
 * @param body The request body, is serialized as JSON.
 */
async function postToDepositMonitor(portAndRoute: string, body: string | object) {
  return axios
    .post('http://127.0.0.1' + portAndRoute, JSON.stringify(body), {
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
      timeout: 5000,
    })
    .then((res) => res.data)
    .catch((error) => {
      let message;
      if (error.response) {
        message = `${error.response.data} (${error.response.status})`;
      } else {
        message = error;
      }
      throw new Error(`Request to deposit monitor (${portAndRoute}) failed: ${message}`);
    });
}

/**
 * Typescript representation of the allowed parameters to `setMockmode`. The JSON encoding of these
 * is what the deposit-monitor expects.
 */
type Mockmode =
  | 'Manual'
  | { Deterministic: { score: number; incomplete_probability: number } }
  | { Random: { min_score: number; max_score: number; incomplete_probability: number } };

/**
 * Set the mockmode of the deposit monitor, controlling how it analyses incoming transactions.
 *
 * @param mode Object describing the mockmode we want to set the deposit-monitor to,
 */
async function setMockmode(mode: Mockmode) {
  return postToDepositMonitor(':6070/mockmode', mode);
}

/**
 * Call the deposit-monitor to set risk score of given transaction in mock analysis provider.
 *
 * @param txid Hash of the transaction we want to report.
 * @param score Risk score for this transaction. Can be in range [0.0, 10.0].
 */
async function setTxRiskScore(txid: string, score: number) {
  await postToDepositMonitor(':6070/riskscore', [
    txid,
    {
      risk_score: { Score: score },
      unknown_contribution_percentage: 0.0,
    },
  ]);
}

/**
 * Checks that the deposit monitor has started up successfully and is healthy.
 */
async function ensureHealth() {
  const response = await postToDepositMonitor(':6060/health', {});
  if (response.starting === true || response.all_processors === false) {
    throw new Error(
      `Deposit monitor is running, but not healthy. It's response was: ${JSON.stringify(response)}`,
    );
  }
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @returns - The the channel id of the deposit channel.
 */
async function brokerLevelScreeningTestScenario(
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  reportFunction: (txId: string) => Promise<void>,
): Promise<string> {
  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };
  const swapParams = await requestNewSwap(
    'Btc',
    'Usdc',
    destinationAddressForUsdc,
    'brokerLevelScreeningTest',
    undefined,
    0,
    true,
    doBoost ? 100 : 0,
    refundParameters,
  );
  const txId = await sendBtcAndReturnTxId(swapParams.depositAddress, amount);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(true);
  }
  await sleep(waitBeforeReport);
  // Note: The bitcoin core js lib returns the txId in reverse order.
  // On chain we expect the txId to be in the correct order (like the Bitcoin internal representation).
  // Because of this we need to reverse the txId before marking it for rejection.
  await markTxForRejection(hexStringToBytesArray(txId).reverse(), 'Bitcoin');
  await sleep(stopBlockProductionFor);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(false);
  }
  await reportFunction(txId);
  return Promise.resolve(swapParams.channelId.toString());
}

async function testBrokerLevelScreeningEthereum(sourceAsset: InternalAsset) {
  testBrokerLevelScreening.log(`Testing broker level screening for Ethereum ${sourceAsset}...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const ethereumRefundAddress = await newAssetAddress('Eth');

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: ethereumRefundAddress,
    minPriceX128: '0',
  };

  const swapParams = await requestNewSwap(
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    'brokerLevelScreeningTestEth',
    undefined,
    0,
    true,
    0,
    refundParameters,
  );

  if (sourceAsset === chainGasAsset('Ethereum')) {
    await send(sourceAsset, swapParams.depositAddress);
    testBrokerLevelScreening.log(`Sent initial ${sourceAsset} tx...`);
    await observeEvent('ethereumIngressEgress:DepositFinalised').event;
    testBrokerLevelScreening.log(`Initial deposit ${sourceAsset} received...`);
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // succesfully to make sure the Deposit contract is deployed.
    await using chainflipApi = await getChainflipApi();
    const broadcastId = await observeSwapEvents(swapParams, chainflipApi);
    if (broadcastId === undefined) {
      throw new Error(`Failed to retrieve broadcastId for ${sourceAsset} swap!`);
    }
    await observeBroadcastSuccess(broadcastId, sourceAsset);
  }

  testBrokerLevelScreening.log(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(sourceAsset, swapParams.depositAddress)).transactionHash as string;
  testBrokerLevelScreening.log(`Sent ${sourceAsset} tx...`);
  const txId = hexStringToBytesArray(txHash);

  await markTxForRejection(txId, 'Ethereum');
  testBrokerLevelScreening.log(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent('ethereumIngressEgress:TransactionRejectedByBroker').event;

  let receivedRefund = false;

  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, ethereumRefundAddress);
    const depositAddressBalance = await getBalance(sourceAsset, swapParams.depositAddress);
    if (refundBalance !== '0' && depositAddressBalance === '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive funds refund to address ${ethereumRefundAddress} within timeout!`,
    );
  }

  testBrokerLevelScreening.log(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

// -- Test suite for broker level screening --
//
// In this tests we are interested in the following scenarios:
//
// 1. No boost and early tx report -> tx is reported early and the swap is refunded.
// 2. Boost and early tx report -> tx is reported early and the swap is refunded.
// 3. Boost and late tx report -> tx is reported late and the swap is not refunded.
async function testBrokerLevelScreeningBitcoin(testBoostedDeposits: boolean = false) {
  const MILLI_SECS_PER_BLOCK = 6000;

  // 0. -- Ensure that deposit monitor is running with manual mocking mode --
  await ensureHealth();
  const previousMockmode = (await setMockmode('Manual')).previous;

  // 1. -- Test no boost and early tx report --
  testBrokerLevelScreening.log('Testing broker level screening for Bitcoin with no boost...');
  let btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, async (txId) =>
    setTxRiskScore(txId, 9.0),
  );

  await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  testBrokerLevelScreening.log(`Marked Bitcoin transaction was rejected and refunded 👍.`);

  if (testBoostedDeposits) {
    // 2. -- Test boost and early tx report --
    testBrokerLevelScreening.log(
      'Testing broker level screening for Bitcoin with boost and a early tx report...',
    );
    btcRefundAddress = await newAssetAddress('Btc');

    await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, async (txId) =>
      setTxRiskScore(txId, 9.0),
    );
    await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;

    if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
      throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
    }
    testBrokerLevelScreening.log(`Marked Bitcoin transaction was rejected and refunded 👍.`);

    // 3. -- Test boost and late tx report --
    // Note: We expect the swap to be executed and not refunded because the tx was reported too late.
    testBrokerLevelScreening.log(
      'Testing broker level screening with boost and a late tx report...',
    );
    btcRefundAddress = await newAssetAddress('Btc');

    const channelId = await brokerLevelScreeningTestScenario(
      '0.2',
      true,
      btcRefundAddress,
      // We wait 12 seconds (2 localnet btc blocks) before we submit the tx.
      // We submit the extrinsic manually in order to ensure that even though it definitely arrives,
      // the transaction is refunded because the extrinsic is submitted too late.
      async (txId) => {
        await sleep(MILLI_SECS_PER_BLOCK * 2);
        await markTxForRejection(txId);
      },
    );

    await observeEvent('bitcoinIngressEgress:DepositFinalised', {
      test: (event) => event.data.channelId === channelId,
    }).event;

    testBrokerLevelScreening.log(`Bitcoin swap was executed and transaction was not refunded 👍.`);
  }

  // 4. -- Restore mockmode --
  await setMockmode(previousMockmode);
}

async function main() {
  await Promise.all([
    testBrokerLevelScreeningBitcoin(),
    testBrokerLevelScreeningEthereum('Eth'),
    testBrokerLevelScreeningEthereum('Usdc'),
  ]);
}

async function main() {
  await Promise.all([
    testBrokerLevelScreeningBitcoin(),
    testBrokerLevelScreeningEthereum('Eth'),
    testBrokerLevelScreeningEthereum('Usdc'),
  ]);
}
