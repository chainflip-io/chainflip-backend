import Client from 'bitcoin-core';
import { sleep, btcClientMutex } from './utils';

export async function sendBtcAndReturnTxId(address: string, amount: number | string): string {
  const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';
  const client = new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'whale',
  });

  // Btc client has a limit on the number of concurrent requests
  const txid: string = await btcClientMutex.runExclusive(async () => {
    let tx: string = await client.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1);
    return tx;
  });

  for (let i = 0; i < 50; i++) {
    const transactionDetails = await client.getTransaction(txid);

    const confirmations = transactionDetails.confirmations;

    if (confirmations < 1) {
      await sleep(1000);
    } else {
      return txid;
    }
  }
  return txid;
}

export async function sendBtc(address: string, amount: number | string) {
  await sendBtcAndReturnTxId(address, amount);
}
