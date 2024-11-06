import { randomBytes } from 'crypto';
import { InternalAsset } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { sendBtc } from '../shared/send_btc';
import { newAddress, sleep, handleSubstrateError, brokerMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';

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
 * Submits a transaction as tainted to the extrinsic on the state chain.
 *
 * @param txId - The txId to submit as tainted as byte array in the order it is on the Bitcoin chain - which
 * is reverse of how it's normally displayed in block explorers.
 */
/* eslint-disable @typescript-eslint/no-unused-vars */
async function submitTxAsTainted(txId: number[]) {
  await using chainflip = await getChainflipApi();
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.bitcoinIngressEgress
      .markTransactionAsTainted(txId)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}

/**
 * Ensures that the deposit-monitor is running and healthy.
 *
 */
async function ensureDepositMonitorHealth() {
  const headers: Headers = new Headers();
  headers.set('Content-Type', 'application/json');
  headers.set('Accept', 'application/json');

  const request: RequestInfo = new Request('http://127.0.0.1:6060/health', {
    method: 'GET',
    headers,
  });

  let responseBody;
  for (let i = 0; i < 10; i++) {
    let res;
    try {
      res = await fetch(request);
    } catch {
      testBrokerLevelScreening.log('Could not connect to deposit monitor, retrying.');
      await sleep(1000);
    }

    if (res) {
      const body = await res.json();

      if (body.starting === false) {
        responseBody = body;
        break;
      } else {
        testBrokerLevelScreening.log('Deposit monitor is starting...');
        await sleep(500);
      }
    }
  }

  if (responseBody === undefined) {
    throw new Error('Could not ensure that deposit monitor is running.');
  }

  const body = responseBody;
  const health =
    body.transaction_processor &&
    body.external_state_processor &&
    body.analysis_processor &&
    body.judgement_processor;
  testBrokerLevelScreening.log('Deposit monitor health: ' + health);
  if (!health) {
    testBrokerLevelScreening.log('Deposit monitor health response is:  ' + JSON.stringify(body));
    throw new Error('Could not ensure that deposit monitor is healthy.');
  }
  return health;
}

/**
 * Call the deposit-monitor to set risk score of given transaction in mock analysis provider.
 * @param txid Hash of the transaction we want to report.
 * @param score Risk score for this transaction. Can be in range [0.0, 10.0].
 */
function setTxRiskScore(txid: string, score: number) {
  const headers: Headers = new Headers();
  headers.set('Content-Type', 'application/json');
  headers.set('Accept', 'application/json');
  const request: RequestInfo = new Request('http://127.0.0.1:6070/riskscore', {
    method: 'POST',
    headers,
    body: JSON.stringify([
      txid,
      {
        risk_score: { Score: score },
        unknown_contribution_percentage: 0.0,
        analysis_provider: 'elliptic_analysis_provider',
      },
    ]),
  });

  return fetch(request).then((res) =>
    testBrokerLevelScreening.log('got response' + JSON.stringify(res)),
  );
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @param waitBeforeReport - The number of milliseconds to wait before reporting the tx as tainted.
 * @returns - The the channel id of the deposit channel.
 */
async function brokerLevelScreeningTestScenario(
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  waitBeforeReport: number,
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
  const txId = await sendBtc(swapParams.depositAddress, amount, 0);
  await sleep(waitBeforeReport);
  await setTxRiskScore(txId, 9.0);
  return swapParams.channelId.toString();
}

// -- Test suite for broker level screening --
//
// In this tests we are interested in the following scenarios:
//
// 1. No boost and early tx report -> Tainted tx is reported early and the swap is refunded.
// 2. Boost and early tx report -> Tainted tx is reported early and the swap is refunded.
// 3. Boost and late tx report -> Tainted tx is reported late and the swap is not refunded.
async function main() {
  const MILLI_SECS_PER_BLOCK = 6000;

  // 0. -- Ensure that deposit monitor is running --
  await ensureDepositMonitorHealth();

  // 1. -- Test no boost and early tx report --
  testBrokerLevelScreening.log('Testing broker level screening with no boost...');
  let btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, 0);

  await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  testBrokerLevelScreening.log(`Tainted transaction was rejected and refunded 👍.`);

  // 2. -- Test boost and early tx report --
  testBrokerLevelScreening.log(
    'Testing broker level screening with boost and a early tx report...',
  );
  btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, 0);
  await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;

  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }
  testBrokerLevelScreening.log(`Tainted transaction was rejected and refunded 👍.`);

  // 3. -- Test boost and late tx report --
  // Note: We expect the swap to be executed and not refunded because the tainted tx was reported too late.
  testBrokerLevelScreening.log('Testing broker level screening with boost and a late tx report...');
  btcRefundAddress = await newAssetAddress('Btc');

  const channelId = await brokerLevelScreeningTestScenario(
    '0.2',
    true,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * 2,
  );

  await observeEvent('bitcoinIngressEgress:DepositFinalised', {
    test: (event) => event.data.channelId === channelId,
  }).event;

  testBrokerLevelScreening.log(`Swap was executed and tainted transaction was not refunded 👍.`);
}
