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
    wallet: 'watch',
});

export async function getBtcBalance(bitcoinAddress: string) {
    console.log(`Getting balance for ${bitcoinAddress}`);
    try {
        const result = await client.listReceivedByAddress(1, false, true, bitcoinAddress);
        const amount = result[0]?.amount || 0;
        console.log(amount);
    } catch (error) {
        console.log(`ERROR: ${error}`);
        process.exit(-1);
    }

    process.exit(0);
}

const bitcoinAddress = process.argv[2];

if (!bitcoinAddress) {
    console.log("Must provide an address to query");
    process.exit(-1);
}

getBtcBalance(bitcoinAddress);