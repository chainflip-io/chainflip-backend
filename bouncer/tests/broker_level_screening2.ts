import axios from 'axios';
import { randomBytes } from 'crypto';
import { InternalAsset } from '@chainflip/cli';
import { sendBtc, sendBtcAndReturnTxId } from '../shared/send_btc';
import {
  newAddress,
  sleep,
  handleSubstrateError,
  brokerMutex,
  hexStringToBytesArray,
  amountToFineAmount,
  assetDecimals,
  chainGasAsset,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';
import { signAndSendTxEvm } from '../shared/send_evm';
import { getBalance } from '../shared/get_balance';
import { send } from '../shared/send';
// import { TestContext } from '../shared/utils/test_context';
// import { Logger } from '../shared/utils/logger';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

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
async function markTxForRejection(txId: string) {
  // The engine uses the memory representation everywhere, so we convert the txId here.
  const memoryRepresentationTxId = hexStringToBytesArray(txId).reverse();
  await using chainflip = await getChainflipApi();
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.bitcoinIngressEgress
      .markTransactionForRejection(memoryRepresentationTxId)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
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

async function setTxRiskScoreEth(txid: string, score: number) {
  await postToDepositMonitor(':6070/riskscore_eth', [
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

async function testBrokerLevelScreeningEthereum(sourceAsset: InternalAsset) {
  console.log('Testing broker level screening with ethereum...');
  const MAX_RETRIES = 30;

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
    console.log('Sent initial tx...');
    await observeEvent('ethereumIngressEgress:DepositFinalised').event;
    console.log('Initial deposit received...');
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit contracts.
    // We will check for a second transaction instead.
    // Sleep until the fetch is broadcasted so the contract is deployed. This is just a
    // temporary patch, we should instead monitor the right broadcast.
    console.log('Sleeping for 60 seconds to wait for the fetch to be broadcasted...');
    await sleep(60000);
  }

  console.log('Sending tx to reject...');
  const txHash = (await send(sourceAsset, swapParams.depositAddress)).transactionHash as string;
  console.log('Sent tx...');
  const txId = hexStringToBytesArray(txHash);

  // const weiAmount = amountToFineAmount('0.002', assetDecimals('Eth'));

  // const receipt = await signAndSendTxEvm(
  //   'Ethereum',
  //   swapParams.depositAddress,
  //   weiAmount,
  //   undefined,
  //   undefined,
  //   true,
  // );


  await setTxRiskScoreEth(txHash, 9.0);
  console.log(`Marked ${txHash} for rejection. Awaiting refund.`);
  // const txId = hexStringToBytesArray(receipt.transactionHash);

  // await markTxForRejection(txId, 'Ethereum');


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

  console.log(`Marked transaction was rejected and refunded 👍.`);

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
  // logger: Logger,
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
    // logger.child({ tag: 'brokerLevelScreeningTest' }),
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
  await reportFunction(txId);
  return swapParams.channelId.toString();
}

// -- Test suite for broker level screening --
//
// In this tests we are interested in the following scenarios:
//
// 1. No boost and early tx report -> tx is reported early and the swap is refunded.
// 2. Boost and early tx report -> tx is reported early and the swap is refunded.
// 3. Boost and late tx report -> tx is reported late and the swap is not refunded.
export async function testBrokerLevelScreening(
  // testContext: TestContext,
  testBoostedDeposits: boolean = false,
) {


  // const logger = testContext.logger;
  const MILLI_SECS_PER_BLOCK = 6000;

  // 0. -- Ensure that deposit monitor is running with manual mocking mode --
  await ensureHealth();
  const previousMockmode = (await setMockmode('Manual')).previous;

  await testBrokerLevelScreeningEthereum('Eth');
  await testBrokerLevelScreeningEthereum('Usdc');
  await testBrokerLevelScreeningEthereum('Usdt');
  await testBrokerLevelScreeningEthereum('Flip');


  /*


  // 1. -- Test no boost and early tx report --
  console.log('Testing broker level screening with no boost...');
  let btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, async (txId) =>
    setTxRiskScore(txId, 9.0),
  );

  await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  console.log(`Marked transaction was rejected and refunded 👍.`);

  // 2. -- Test boost and early tx report --
  if (testBoostedDeposits) {
    // 2. -- Test boost and early tx report --
    console.log('Testing broker level screening with boost and a early tx report...');
    btcRefundAddress = await newAssetAddress('Btc');

    await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, async (txId) =>
      setTxRiskScore(txId, 9.0),
    );
    await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;

    if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
      throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
    }
    console.log(`Marked transaction was rejected and refunded 👍.`);

    // // 3. -- Test boost and late tx report --
    // // Note: We expect the swap to be executed and not refunded because the tx was reported too late.
    // logger.debug('Testing broker level screening with boost and a late tx report...');
    // btcRefundAddress = await newAssetAddress('Btc');

    // const channelId = await brokerLevelScreeningTestScenario(
    //   logger,
    //   '0.2',
    //   true,
    //   btcRefundAddress,
    //   // We wait 12 seconds (2 localnet btc blocks) before we submit the tx.
    //   // We submit the extrinsic manually in order to ensure that even though it definitely arrives,
    //   // the transaction is refunded because the extrinsic is submitted too late.
    //   async (txId) => {
    //     await sleep(MILLI_SECS_PER_BLOCK * 2);
    //     await markTxForRejection(txId);
    //   },
    // );

    // await observeEvent(logger, 'bitcoinIngressEgress:DepositFinalised', {
    //   test: (event) => event.data.channelId === channelId,
    // }).event;

    // logger.debug(`Swap was executed and transaction was not refunded 👍.`);
  }

  */

  // 4. -- Restore mockmode --
  await setMockmode(previousMockmode);
}
