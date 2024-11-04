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

/** Tests the broker level screening mechanism.
 * 
 * @param amount - The amount of BTC to send.
 * @param doBoost - Whether to boost the swap.
 * @param stopBlockProductionFor - The number of milliseconds to pause block production for.
 */
async function brokerLevelScreeningTestScenario(amount: string, doBoost: boolean, refundAddress: string, stopBlockProductionFor = 0, waitBeforeReport = 0) {
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
    let tx_id = await sendBtcAndReturnTxId(swapParams.depositAddress, amount);
    if (stopBlockProductionFor > 0) {
        pauseBtcBlockProduction(true);
    }
    await sleep(waitBeforeReport);
    await submitTxAsTainted(hexStringToBytesArray(tx_id).reverse());
    await sleep(stopBlockProductionFor);
    if (stopBlockProductionFor > 0) {
        pauseBtcBlockProduction(false);
    }
}

async function main() {
    const MILLI_SECS_PER_BLOCK = 6000;
    const BLOCKS_TO_WAIT = 2

    // Test no boost and early tx report
    testBrokerLevelScreening.log('Testing broker level screening with no boost...');
    let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    await brokerLevelScreeningTestScenario('0.2', false, btcRefundAddress, MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT);
    await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
    let btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
    testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
    testBrokerLevelScreening.log(`Tx was rejected and refunded 👍.`);

    // Test boost and early tx report
    testBrokerLevelScreening.log('Testing broker level screening with boost and a early tx report...');
    btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT);
    await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
    btcBalance = await observeBtcAddressBalanceChange(btcRefundAddress);
    testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
    testBrokerLevelScreening.log(`Tx was rejected and refunded 👍.`);

    // Test boost and late tx report
    testBrokerLevelScreening.log('Testing broker level screening with boost and a late tx report...');
    btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, MILLI_SECS_PER_BLOCK * 2);
    await observeEvent('swapping:SwapExecuted').event;
    testBrokerLevelScreening.log(`BTC balance: ${btcBalance}`);
    testBrokerLevelScreening.log(`Swap was executed 👍.`);
}
