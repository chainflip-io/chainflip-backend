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

export async function fundAndSendTransaction(
  outputs: object[],
  changeAddress: string,
  feeRate?: number,
): Promise<string> {
  return btcClientMutex.runExclusive(async () => {
    const rawTx = await btcClient.createRawTransaction([], outputs);
    const fundedTx = (await btcClient.fundRawTransaction(rawTx, {
      changeAddress,
      feeRate: feeRate ?? 0.00001,
      lockUnspents: true,
      changePosition: 2,
    })) as { hex: string };
    const signedTx = await btcClient.signRawTransactionWithWallet(fundedTx.hex);
    const txId = (await btcClient.sendRawTransaction(signedTx.hex)) as string | undefined;

    if (!txId) {
      throw new Error('Broadcast failed');
    }

    return txId;
  });
}

export async function sendVaultTransaction(
  nulldataPayload: string,
  amountBtc: number,
  depositAddress: string,
  refundAddress: string,
): Promise<string> {
  return fundAndSendTransaction(
    [
      {
        [depositAddress]: amountBtc,
      },
      {
        data: nulldataPayload.replace('0x', ''),
      },
    ],
    refundAddress,
  );
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
