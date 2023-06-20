import Module from "node:module";
const require = Module.createRequire(import.meta.url);

const Client = require('bitcoin-core');

const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';
console.log(`BTC_ENDPOINT is set to '${BTC_ENDPOINT}'`);

const client = new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'whale',
});


export async function fundBtc(address: string, amount: any) {
    const txid = await client.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1);

    for (let i = 0; i < 50; i++) {
        const transactionDetails = await client.getTransaction(txid);

        const confirmations = transactionDetails.confirmations;

        if (confirmations < 1) {
            await new Promise(resolve => setTimeout(resolve, 1000));
        }
    }
}