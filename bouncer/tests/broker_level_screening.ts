import { jsonRpc } from '../shared/json_rpc';
import { sendBtc, sendBtcAndReturnTxIdWithoutWaiting } from '../shared/send_btc';
import EventSource from 'eventsource';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function brokerApiRpc(method: string, params: any[]): Promise<any> {
  return jsonRpc(method, params, 'http://127.0.0.1:10997');
}


function setTxRiskScore(tx_id: unknown, score: number) {

  // We can use the `Headers` constructor to create headers
  // and assign it as the type of the `headers` variable
  const headers: Headers = new Headers()
  // Add a few headers
  headers.set('Content-Type', 'application/json')
  headers.set('Accept', 'application/json')
  // Add a custom header, which we can use to check
//   headers.set('X-Custom-Header', 'CustomValue')

  // Create the request object, which will be a RequestInfo type. 
  // Here, we will pass in the URL as well as the options object as parameters.
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



  // For our example, the data is stored on a static `users.json` file
  return fetch(request)
    // the JSON body is taken from the response
    .then(res => testBrokerLevelScreening.log("got response" + res));

    

    // await using chainflip = await brokerApiRpc();
    // return brokerMutex.runExclusive(async () =>
    //     chainflip.tx.bitcoinIngressEgress
    //         .markTransactionAsTainted(tx_id)
    //         .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
    // );
}


async function test_monitoring() {
    // stop btc block creation

    // open swap channel

    // txid = create new bitcoin transaction

    // submit judgement for txid risk_score = 9.0

    // mine next bitcoin block manually

    // ensure that monitoring has submitted sucessfully

    // reenable block creation
}





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

async function submitTxAsTainted(tx_id: unknown) {
    await using chainflip = await getChainflipApi();
    return brokerMutex.runExclusive(async () =>
        chainflip.tx.bitcoinIngressEgress
            .markTransactionAsTainted(tx_id)
            .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
    );
}

function pauseBtcBlockProduction(command: boolean): boolean {
    let start = "docker exec bitcoin rm /root/mine_blocks";
    let stop = "docker exec bitcoin touch /root/mine_blocks";
    try {
        execSync(command ? start : stop);
        return true;
    } catch (error) {
        console.error(error);
        return false;
    }
}

// type DepositMonitorScenario = {
//     amount: string,
//     boost: boolean,
//     riskScore: number
//     refundAddress: string,
// }

/** Tests the broker level screening mechanism.
 * 
 * @param amount - The amount of BTC to send.
 * @param doBoost - Whether to boost the swap.
 * @param stopBlockProductionFor - The number of milliseconds to pause block production for.
 */
async function brokerLevelScreeningTestScenario(amount: string, doBoost: boolean, refundAddress: string, riskScore: number, stopBlockProductionFor = 0, waitBeforeReport = 0) {

    // testBrokerLevelScreening.log("stopping block production...");
    // if (stopBlockProductionFor > 0) {
    //     pauseBtcBlockProduction(true);
    // }
    // await sleep(500);

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
    testBrokerLevelScreening.log("creating new tx to address: " + swapParams.depositAddress);
    testBrokerLevelScreening.log("... and refund address: " + refundAddress);
    let tx_id = await sendBtcAndReturnTxIdWithoutWaiting(swapParams.depositAddress, amount);

    testBrokerLevelScreening.log("created and sent new tx with id: " + tx_id);

    setTxRiskScore(tx_id, riskScore);

    testBrokerLevelScreening.log("setting risk score done, waiting for CFDM to process");

    // await awaitDmEvent((data) => {
    //     return (data.indexOf("PendingRefundConfirmation") >= 0)
    // });
    // await sleep(30000);

    if (stopBlockProductionFor > 0) {
        pauseBtcBlockProduction(false);
    }

    testBrokerLevelScreening.log("restarted block production");
}

async function awaitDmEvent(f: (a0: String) => boolean) {
    const evtSource = new EventSource("http://localhost:6060/events");
    let got_event = false;
    evtSource.addEventListener("mylistener", (event) => {
        testBrokerLevelScreening.log("got event: " + event.data);
        if (f(event.data)) {
            got_event = true;
        }
        return;
    });

    for (let i = 0; i < 5; i++) {
        console.log("waiting for event...");
        await sleep(1000);
    }
}


async function main() {

    var es = new EventSource('http://localhost:6060/events');
    es.addEventListener('my-events', function(e) {
        console.log(e.data);
    });

    const MILLI_SECS_PER_BLOCK = 6000;
    const BLOCKS_TO_WAIT = 2

    // Test no boost and early tx report
    // testBrokerLevelScreening.log('Testing broker level screening with no boost...');
    // let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    // await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT);
    // await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
    // let btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
    // testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
    // testBrokerLevelScreening.log(`Tx was rejected and refunded 👍.`);

    // Test boost and early tx report
    testBrokerLevelScreening.log('Testing broker level screening with boost');
    let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, 9.0, MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT);
    await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
    let btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
    testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
    testBrokerLevelScreening.log(`Tx was rejected and refunded 👍.`);

    // // Test boost and late tx report
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

