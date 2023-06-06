import Module from "node:module";
const require = Module.createRequire(import.meta.url);

const Client = require('bitcoin-core');

const bitcoin_address = process.argv[2];
const btc_amount = parseFloat(process.argv[3]);

const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';
console.log(`BTC_ENDPOINT is set to '${BTC_ENDPOINT}'`);

const client = new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'whale',
});

async function sendBitcoin() {
    try {
        const txid = await client.sendToAddress(bitcoin_address, btc_amount, '', '', false, true, null, 'unset', null, 1);

        for (let i = 0; i < 50; i++) {
            const transactionDetails = await client.getTransaction(txid);

            let confirmations = transactionDetails.confirmations;

            if (confirmations < 1) {
                await new Promise(resolve => setTimeout(resolve, 1000));
            } else {
                process.exit(0);
            }
        }


    } catch (error) {
        console.log(`ERROR: ${error}`);
        process.exit(-1);
    }
}

sendBitcoin();