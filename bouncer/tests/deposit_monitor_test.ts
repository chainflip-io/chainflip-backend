import { randomBytes } from 'crypto';
import { execSync } from 'child_process';
import { InternalAsset } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { sendBtcAndReturnTxId } from '../shared/send_btc';
import {
  hexStringToBytesArray,
  newAddress,
  sleep,
  handleSubstrateError,
  brokerMutex,
} from '../shared/utils';
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
async function submitTxAsTainted(txId: number[]) {
  await using chainflip = await getChainflipApi();
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.bitcoinIngressEgress
      .markTransactionAsTainted(txId)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}

/**
 * Pauses or resumes the bitcoin block production. We send a command to the docker container to start or stop mining blocks.
 *
 * @param pause - Whether to pause or resume the block production.
 * @returns - Whether the command was successful.
 */
function pauseBtcBlockProduction(pause: boolean): boolean {
  try {
    execSync(
      pause
        ? 'docker exec bitcoin rm /root/mine_blocks'
        : 'docker exec bitcoin touch /root/mine_blocks',
    );
    return true;
  } catch (error) {
    console.error(error);
    return false;
  }
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @param stopBlockProductionFor - The number of blocks to stop block production for. We need this to ensure that the tainted tx is on chain before the deposit is witnessed/prewitnessed.
 * @param waitBeforeReport - The number of milliseconds to wait before reporting the tx as tainted.
 * @returns - The the channel id of the deposit channel.
 */
async function brokerLevelScreeningTestScenario(
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  stopBlockProductionFor = 0,
  waitBeforeReport = 0,
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
  // Because of this we need to reverse the txId before submitting it as tainted.
  await submitTxAsTainted(hexStringToBytesArray(txId).reverse());
  await sleep(stopBlockProductionFor);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(false);
  }
  return Promise.resolve(swapParams.channelId.toString());
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
  const BLOCKS_TO_WAIT = 2;

  // 1. -- Test no boost and early tx report --
  testBrokerLevelScreening.log('Testing broker level screening with no boost...');
  let btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario(
    '0.2',
    false,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );

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

  await brokerLevelScreeningTestScenario(
    '0.2',
    true,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );
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
    0,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );

  await observeEvent('bitcoinIngressEgress:DepositFinalised', {
    test: (event) => event.data.channelId === channelId,
  }).event;

  testBrokerLevelScreening.log(`Swap was executed and tainted transaction was not refunded 👍.`);
}







/*

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

*/