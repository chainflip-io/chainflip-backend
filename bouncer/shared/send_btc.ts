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

  // Prepare the transaction inputs
  const inputs = [
    {
      txid: utxo.txid,
      vout: utxo.vout,
    },
  ];

  return {
    inputs,
    change,
  };
}

export async function sendVaultTransaction(
  nulldataUtxo: string,
  amountBtc: number,
  depositAddress: string,
  refundAddress: string,
) {
  return btcClientMutex.runExclusive(async () => {
    const feeBtc = 0.00001;
    const { inputs, change } = await selectInputs(Number(amountBtc) + feeBtc);

    // The `createRawTransaction` function will add the op codes, so we have to remove them here.
    const nullDataWithoutOpCodes = nulldataUtxo.replace('0x', '').substring(4);

    const outputs = [
      {
        [depositAddress]: amountBtc,
      },
      {
        data: nullDataWithoutOpCodes,
      },
      {
        [refundAddress]: change,
      },
    ];

    const rawTx = await btcClient.createRawTransaction(inputs, outputs, 0, false);
    const signedTx = await btcClient.signRawTransactionWithWallet(rawTx);
    const txid = await btcClient.sendRawTransaction(signedTx.hex);

    if (!txid) {
      throw new Error('Broadcast failed');
    }
    return txid as string;
  });
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
  throw new Error(`Timeout waiting for Btc transaction to be confirmed, txid: ${txid}`);
}

export async function sendBtc(
  address: string,
  amount: number | string,
  confirmations = 1,
): Promise<string> {
  // Btc client has a limit on the number of concurrent requests
  const txid = (await btcClientMutex.runExclusive(async () =>
    btcClient.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1),
  )) as string;

  if (confirmations > 0) {
    await waitForBtcTransaction(txid, confirmations);
  }

  return txid;
}
