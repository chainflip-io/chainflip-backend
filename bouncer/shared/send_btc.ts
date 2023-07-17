import Client from 'bitcoin-core';
import { sleep, btcClientMutex } from './utils';

export async function sendBtc(address: string, amount: number | string) {
  const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';
  const client = new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'whale',
  });

  // Give the CFE time to start monitoring the new BTC address before we send funds to it
  // This is only necessary on partnernet bouncer, because of the 1 block safety margin for BTC
  await sleep(10000);

  // BTC client has a limit on the number of concurrent requests
  const txid = await btcClientMutex.runExclusive(async () =>
    client.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1),
  );

  for (let i = 0; i < 50; i++) {
    const transactionDetails = await client.getTransaction(txid);

    const confirmations = transactionDetails.confirmations;

    if (confirmations < 1) {
      await sleep(1000);
    } else {
      return;
    }
  }
}
