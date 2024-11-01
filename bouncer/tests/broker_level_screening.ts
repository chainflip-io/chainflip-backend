import { ExecutableTest } from '../shared/executable_test';
import { sendBtcAndReturnTxId, sendBtcFireAndForget } from '../shared/send_btc';
import { randomBytes } from 'crypto';
import { hexStringToBytesArray, newAddress, sleep } from '../shared/utils';
import { handleSubstrateError } from '../shared/utils';
import { brokerMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { stringToU8a } from '@polkadot/util'
import { execSync } from 'child_process';

const keyring = new Keyring({ type: 'sr25519' });

export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 300);

const broker = keyring.createFromUri('//BROKER_1');

export async function submitTxAsTainted(tx_id: unknown) {
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

async function markTxAndExpectARefund() {
    const BTC_AMOUNT = '100';
    let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    let destinationAddressForUsdc = await newAddress('Usdc', randomBytes(32).toString('hex'));
    const refundParameters: FillOrKillParamsX128 = {
        retryDurationBlocks: 0,
        refundAddress: btcRefundAddress,
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
        0,
        refundParameters,
    );
    console.log(`Paused block production: ${pauseBtcBlockProduction(true)}`);
    let tx_id = await sendBtcAndReturnTxId(swapParams.depositAddress, BTC_AMOUNT);
    console.log(`Btc tx_id: ${tx_id}`);
    console.log(`Deposit address: ${swapParams.depositAddress}`);
    let tx_id_u8a = hexStringToBytesArray(tx_id);
    console.log(`Tx_id_u8a: ${tx_id_u8a}`);
    await submitTxAsTainted(tx_id_u8a);
    await sleep(6000);
    console.log(`Resumed block production: ${pauseBtcBlockProduction(false)}`);
    console.log('Waiting for tx to be refunded');
    const txRefunded = await observeEvent('bitcoinIngressEgress:DepositIgnored').event;
    console.log(`Tx refunded: ${txRefunded}`);
}

async function main() {
    const BTC_AMOUNT = '12';
    const MILLI_SECS_PER_BLOCK = 6000;
    const BLOCKS_TO_WAIT = 6
    let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));
    let destinationAddressForUsdc = await newAddress('Usdc', randomBytes(32).toString('hex'));
    const refundParameters: FillOrKillParamsX128 = {
        retryDurationBlocks: 0,
        refundAddress: btcRefundAddress,
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
        0,
        refundParameters,
    );
    let tx_id = await sendBtcAndReturnTxId(swapParams.depositAddress, BTC_AMOUNT);
    console.log(`Paused block production: ${pauseBtcBlockProduction(true)}`);
    console.log(`Btc tx_id: ${tx_id}`);
    console.log(`Deposit address: ${swapParams.depositAddress}`);
    let tx_id_u8a = hexStringToBytesArray(tx_id);
    // console.log(`Tx_id_u8a: ${tx_id_u8a}`);
    await submitTxAsTainted(tx_id_u8a);
    await sleep(MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT);
    console.log(`Resumed block production: ${pauseBtcBlockProduction(false)}`);
    const txRefunded = await observeEvent('bitcoinIngressEgress:DepositIgnored').event;
    console.log(`Tx refunded 👍: ${txRefunded}`);
}
