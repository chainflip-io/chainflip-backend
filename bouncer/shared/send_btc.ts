import Client from 'bitcoin-core';
import { sleep, btcClientMutex } from './utils';

export const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';

export const btcClient = new Client({
  host: BTC_ENDPOINT.split(':')[1].slice(2),
  port: Number(BTC_ENDPOINT.split(':')[2]),
  username: 'flip',
  password: 'flip',
  wallet: 'whale',
});

export async function selectInputs(amount: number) {
  // List unspent UTXOs
  const utxos = await btcClient.listUnspent();

  // Find a UTXO with enough funds
  const utxo = utxos.find((u) => u.amount >= amount);
  if (!utxo) throw new Error('Insufficient funds');
  // TODO: be able to select more than one UTXO

  const change = utxo.amount - amount;

  // Prepare the transaction inputs and outputs
  const inputs = [
    {
      txid: utxo.txid,
      vout: utxo.vout,
    },
  ];

  return { inputs, change };
}

export async function waitForBtcTransaction(txid: string, confirmations = 1) {
  for (let i = 0; i < 50; i++) {
    const transactionDetails = await btcClient.getTransaction(txid);

    if (transactionDetails.confirmations < confirmations) {
      await sleep(1000);
    } else {
      return;
    }
  }
}

export async function sendBtc(address: string, amount: number | string) {
  // Btc client has a limit on the number of concurrent requests
  const txid = (await btcClientMutex.runExclusive(async () =>
    btcClient.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1),
  )) as string;

  await waitForBtcTransaction(txid);
}
