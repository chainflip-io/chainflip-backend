import Client from 'bitcoin-core';
import { sleep, btcClientMutex } from './utils';
import { Logger, throwError } from './utils/logger';

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

export async function waitForBtcTransaction(logger: Logger, txid: string, confirmations = 1) {
  logger.trace(
    `Waiting for Btc transaction to be confirmed, txid: ${txid}, required confirmations: ${confirmations}`,
  );
  for (let i = 0; i < 50; i++) {
    const transactionDetails = await btcClient.getTransaction(txid);

    if (transactionDetails.confirmations < confirmations) {
      await sleep(1000);
    } else {
      logger.trace(`Btc transaction confirmed, txid: ${txid}`);
      return;
    }
  }
  throwError(
    logger,
    new Error(`Timeout waiting for Btc transaction to be confirmed, txid: ${txid}`),
  );
}

export async function sendBtc(
  logger: Logger,
  address: string,
  amount: number | string,
  confirmations = 1,
): Promise<string> {
  // Btc client has a limit on the number of concurrent requests
  const txid = (await btcClientMutex.runExclusive(async () =>
    btcClient.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1),
  )) as string;

  if (confirmations > 0) {
    await waitForBtcTransaction(logger, txid, confirmations);
  }

  return txid;
}


export async function sendBtcChain(
  logger: Logger,
  address: string,
  amount: number,
  confirmations = 1,
): Promise<string> {
  // Btc client has a limit on the number of concurrent requests
  const txid = (await btcClientMutex.runExclusive(async () => {
      const intermediate_address = await btcClient.getNewAddress();

      // bitcoin has 8 decimal places
      const stringAmount = (amount * 1.1).toFixed(8);

      let result = undefined;
      try {
      result = await btcClient.sendToAddress(intermediate_address, stringAmount, '', '', false, true, null, 'unset', null, 1);
      } catch (err) {
        throw new Error(`sendToAddress returned : ${err}`)
      }
      const intermediate_txid = result;

      let rawTx = undefined;
      try {
        // Create the raw transaction
        rawTx = await btcClient.createRawTransaction(
          [
            {
              "txid": intermediate_txid as string,
              "vout": 0
            },
          ], 
          {
          [address]: amount,
        });
      } catch (err) {
        throw new Error(`CreateRawTransaction returned : ${err}`)
      }

      let fundedTx = undefined;

      try {
      fundedTx = (await btcClient.fundRawTransaction(rawTx, {
        changeAddress: await btcClient.getNewAddress(),
        feeRate: 0.00001,
        lockUnspents: true,
      })) as { hex: string };
      } catch (err) {
        throw new Error(`fundRawTransaction returned : ${err}`)
      }

      // Sign the raw transaction
      let signedTx = undefined;
      try {
      signedTx = await btcClient.signRawTransactionWithWallet(fundedTx.hex);
      } catch (err) {
        throw new Error(`signRawTransaction returned : ${err}`)
      }

      // Send the signed tx
      let txId = undefined;
      try {
      txId = (await btcClient.sendRawTransaction(signedTx.hex)) as string | undefined;
      } catch (err) {
        throw new Error(`sendRawTransaction returned : ${err}`)
      }

      return intermediate_txid;

  }
  )) as string;

  if (confirmations > 0) {
    await waitForBtcTransaction(logger, txid, confirmations);
  }

  return txid;
}