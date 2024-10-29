import { ExecutableTest } from '../shared/executable_test';
import { sendBtcAndReturnTxId } from '../shared/send_btc';
import { randomBytes } from 'crypto';
import { newAddress } from '../shared/utils';
import { handleSubstrateError } from '../shared/utils';
import { brokerMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';

const keyring = new Keyring({ type: 'sr25519' });

export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 300);

const broker = keyring.createFromUri('//BROKER_FEE_TEST');

export async function submitTxAsTainted(tx_id: unknown) {
    await using chainflip = await getChainflipApi();
    return brokerMutex.runExclusive(async () =>
        chainflip.tx.bitcoinIngressEgress
            .markTransactionAsTainted(tx_id)
            .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
    );
}

async function main() {
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
    let tx_id = await sendBtcAndReturnTxId(swapParams.depositAddress, BTC_AMOUNT);
    console.log(`Btc tx_id: ${tx_id}`);
    await submitTxAsTainted(tx_id);
    console.log('Waiting for tx to be refunded');
    const txRefunded = await observeEvent('bitcoinIngressEgress:DepositIgnored').event;
    console.log(`Tx refunded: ${txRefunded}`);
}
