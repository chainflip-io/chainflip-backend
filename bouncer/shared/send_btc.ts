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

/**
 * Creates a chain of 2 btc transactions (parent & child tx)
 * 
 * @param logger - Logger to use
 * @param address - Target address of the child tx
 * @param amount - Amount of the child tx
 * @param parentConfirmations - How many blocks to wait after the parent tx before sending the child tx
 * @param childConfirmations - How many blocks to wait after the child tx before returning
 * @returns - The txids of the parent and child tx
 */
export async function sendBtcTransactionWithParent(
  logger: Logger,
  address: string,
  amount: number,
  parentConfirmations: number,
  childConfirmations: number,
): Promise<{parentTxid: string, childTxid: string}> {
  // Btc client has a limit on the number of concurrent requests
  const txids = (await btcClientMutex.runExclusive(async () => {

    // create a new address in our wallet that we have the keys for
    const intermediateAddress = await btcClient.getNewAddress();

    // amount to use for the parent tx
    // Note: bitcoin has 8 decimal places
    const parentAmount = (amount * 1.1).toFixed(8);

    // send the parent tx
    const parentTxid = await btcClient.sendToAddress(intermediateAddress, parentAmount, '', '', false, true, null, 'unset', null, 1) as string;

    // wait for inclusion in a block
    if (parentConfirmations > 0) {
      await waitForBtcTransaction(logger, parentTxid, parentConfirmations);
    }

    // Create a raw transaction for the child tx
    const childRawTx = await btcClient.createRawTransaction(
        [
          {
            "txid": parentTxid as string,
            "vout": 1
          },
        ], 
        {
        [address]: amount,
      });

    // Fund the child tx
    const childFundedTx = (await btcClient.fundRawTransaction(childRawTx, {
      changeAddress: await btcClient.getNewAddress(),
      feeRate: 0.00001,
      lockUnspents: true,
    })) as { hex: string };

    // Sign the child tx
    const childSignedTx = await btcClient.signRawTransactionWithWallet(childFundedTx.hex);

    // Send the signed tx
    const childTxid = (await btcClient.sendRawTransaction(childSignedTx.hex)) as string;

    return { parentTxid: parentTxid, childTxid: childTxid};
  }
  ));

  if (childConfirmations > 0) {
    await waitForBtcTransaction(logger, txids.childTxid, childConfirmations);
  }

  return txids;
}