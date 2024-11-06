import { jsonRpc } from '../shared/json_rpc';
import { sendBtc, sendBtcAndReturnTxIdWithoutWaiting } from '../shared/send_btc';
import { ExecutableTest } from '../shared/executable_test';
import { sendBtcAndReturnTxId } from '../shared/send_btc';
import { randomBytes } from 'crypto';
import { hexStringToBytesArray, newAddress, sleep } from '../shared/utils';
import { handleSubstrateError } from '../shared/utils';
import { brokerMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { execSync } from 'child_process';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';
import { HttpStatusCode } from 'axios';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function brokerApiRpc(method: string, params: any[]): Promise<any> {
  return jsonRpc(method, params, 'http://127.0.0.1:10997');
}

function setTxRiskScore(tx_id: unknown, score: number) {

  const headers: Headers = new Headers()
  headers.set('Content-Type', 'application/json')
  headers.set('Accept', 'application/json')
  const request: RequestInfo = new Request('http://127.0.0.1:6070/riskscore', {
    method: 'POST',
    headers: headers,
    body: JSON.stringify(
        [
            tx_id,
            {"risk_score": {"Score": score}, "unknown_contribution_percentage": 0.0, "analysis_provider": "elliptic_analysis_provider"}
        ]
    )
  })

  return fetch(request)
    .then(res => testBrokerLevelScreening.log("got response" + JSON.stringify(res)));
}

async function ensureDepositMonitorHealth() {
  const headers: Headers = new Headers()
  headers.set('Content-Type', 'application/json')
  headers.set('Accept', 'application/json')

  const request: RequestInfo = new Request('http://127.0.0.1:6060/health', {
    method: 'GET',
    headers: headers,
  })

  let response_body = undefined;
  for (let i = 0; i < 30; i++) {
    let res = undefined;
    try {
        res = await fetch(request);
    } catch {
        testBrokerLevelScreening.log("Could not connect to deposit monitor, retrying.");
        await sleep(1000);
        continue;
    }
    const body = await res.json();

    if (body.starting === false) {
        response_body = body;
        break;
    } else {
        testBrokerLevelScreening.log("Deposit monitor is starting...");
        await sleep(500);
    }
  }

  if (response_body === undefined) {
    throw new Error("Could not ensure that deposit monitor is running.")
  }

  const body = response_body;
  const health = body.transaction_processor && body.external_state_processor && body.analysis_processor && body.judgement_processor;
  testBrokerLevelScreening.log("Deposit monitor health: " + health);
  if (!health) {
    testBrokerLevelScreening.log("Deposit monitor health response is:  " + JSON.stringify(body));
    throw new Error("Could not ensure that deposit monitor is healthy.");
  }
  return health;
}



const keyring = new Keyring({ type: 'sr25519' });

export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 300);

const broker = keyring.createFromUri('//BROKER_1');

async function observeBtcAddressBalanceChange(address: string): Promise<number> {
    let retryCount = 0;
    while (true) {
        await sleep(1000);
        let balance = await getBtcBalance(address);
        if (balance !== 0) {
            return Promise.resolve(balance);
        }
        testBrokerLevelScreening.log("waiting for refund to arrive, retry: " + retryCount);
        retryCount++;
        if (retryCount > 100) {
            throw new Error(`BTC balance for ${address} did not change after 16 seconds.`);
        }
    }
}



/** Tests the broker level screening mechanism.
 * 
 * @param amount - The amount of BTC to send.
 * @param doBoost - Whether to boost the swap.
 * @param stopBlockProductionFor - The number of milliseconds to pause block production for.
 */
async function brokerLevelScreeningTestScenario(amount: string, doBoost: boolean, refundAddress: string, riskScore: number, waitBeforeReport = 0) {

    let destinationAddressForUsdc = await newAddress('Usdc', randomBytes(32).toString('hex'));
    const refundParameters: FillOrKillParamsX128 = {
        retryDurationBlocks: 0,
        refundAddress: refundAddress,
        minPriceX128: '0',
    };
    const swapParams = await requestNewSwap(
        'Btc',
        'Usdc',
        destinationAddressForUsdc,
        'test',
        undefined,
        0,
        true,
        doBoost ? 100 : 0,
        refundParameters,
    );
    testBrokerLevelScreening.log("Creating new tx...");
    let tx_id = await sendBtcAndReturnTxIdWithoutWaiting(swapParams.depositAddress, amount);

    testBrokerLevelScreening.log("Created and sent new tx with id: " + tx_id);

    if (waitBeforeReport > 0) {
        testBrokerLevelScreening.log("Waiting before submitting risk score..." );
        await sleep(waitBeforeReport);
    }
    setTxRiskScore(tx_id, riskScore);

    testBrokerLevelScreening.log("setting risk score done, waiting for CFDM to process");
}



async function main() {
    await ensureDepositMonitorHealth();

    const MILLI_SECS_PER_BLOCK = 6000;
    const BLOCKS_TO_WAIT = 2

    {
        // Test no boost and early tx report
        testBrokerLevelScreening.log('Testing broker level screening with no boost...');
        let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
        await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, 9.0, 5000);
        await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
        let btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
        testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
        testBrokerLevelScreening.log(`Tx was rejected and refunded 👍.`);
    }

    {
        // Test boost and early tx report
        testBrokerLevelScreening.log('Testing broker level screening with boost');
        let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
        await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, 9.0, 0);
        await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
        let btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
        testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
        testBrokerLevelScreening.log(`Tx was rejected and refunded 👍.`);
    }

    // Test boost and late tx report
    {
        testBrokerLevelScreening.log('Testing broker level screening without boost and with low risk score');
        let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
        await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, 1.7, MILLI_SECS_PER_BLOCK * 2);
        await observeEvent('swapping:SwapExecuted').event;
        // let btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
        // testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
        testBrokerLevelScreening.log(`Swap was executed 👍.`);
    }
}

