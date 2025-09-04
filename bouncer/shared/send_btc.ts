import Client from 'bitcoin-core';
import * as bitcoinjs from 'bitcoinjs-lib';
import * as ecc from 'tiny-secp256k1';
import { sleep, btcClientMutex } from 'shared/utils';
import { Logger, throwError } from 'shared/utils/logger';

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

export async function waitForBtcTransaction(
  logger: Logger,
  txid: string,
  confirmations = 1,
  client = btcClient,
) {
  logger.trace(
    `Waiting for Btc transaction to be confirmed, txid: ${txid}, required confirmations: ${confirmations}`,
  );
  // Localnet btc blocktime is 5sec
  const timeoutSeconds = 10 + 5 * confirmations;
  for (let i = 0; i < timeoutSeconds; i++) {
    const transactionDetails = await client.getTransaction(txid);

    if (transactionDetails.confirmations < confirmations) {
      await sleep(1000);
    } else {
      logger.trace(`Btc transaction confirmed, txid: ${txid} in ${i} seconds`);
      return;
    }
  }
  throwError(
    logger,
    new Error(
      `Timeout (${timeoutSeconds}s) waiting for Btc transaction to be confirmed, txid: ${txid}`,
    ),
  );
}

export async function sendBtc(
  logger: Logger,
  address: string,
  amount: number | string,
  confirmations = 1,
  client = btcClient,
): Promise<string> {
  // Btc client has a limit on the number of concurrent requests
  let txid: string;
  let attempts = 0;
  const maxAttempts = 3;

  while (attempts < maxAttempts) {
    try {
      txid = (await btcClientMutex.runExclusive(async () =>
        client.sendToAddress(address, amount, '', '', false, true, null, 'unset', null, 1),
      )) as string;

      if (confirmations > 0) {
        await waitForBtcTransaction(logger, txid, confirmations, client);
      }
      return txid;
    } catch (error) {
      attempts++;
      logger.warn(`Error sending BTC transaction (attempt ${attempts}): ${error}`);
      if (attempts >= maxAttempts) {
        throw error;
      }
      await sleep(1000);
    }
  }

  return '';
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
  client = btcClient,
): Promise<{ parentTxid: string; childTxid: string }> {
  // Btc client has a limit on the number of concurrent requests
  const txids = await btcClientMutex.runExclusive(async () => {
    // create a new address in our wallet that we have the keys for
    const intermediateAddress = await client.getNewAddress();

    // amount to use for the parent tx
    // Note: bitcoin has 8 decimal places
    const parentAmount = (amount * 1.1).toFixed(8);

    // send the parent tx
    const parentTxid = (await client.sendToAddress(
      intermediateAddress,
      parentAmount,
      '',
      '',
      false,
      true,
      null,
      'unset',
      null,
      1,
    )) as string;

    // wait for inclusion in a block
    if (parentConfirmations > 0) {
      await waitForBtcTransaction(logger, parentTxid, parentConfirmations, client);
    }

    // Create a raw transaction for the child tx
    const childRawTx = await client.createRawTransaction(
      [
        {
          txid: parentTxid as string,
          vout: 0,
        },
      ],
      {
        [address]: amount,
      },
    );

    // Fund the child tx
    const childFundedTx = (await client.fundRawTransaction(childRawTx, {
      changeAddress: await client.getNewAddress(),
      feeRate: 0.00001,
      lockUnspents: true,
    })) as { hex: string };

    // Sign the child tx
    const childSignedTx = await client.signRawTransactionWithWallet(childFundedTx.hex);

    // Send the signed tx
    const childTxid = (await client.sendRawTransaction(childSignedTx.hex)) as string;

    return { parentTxid, childTxid };
  });

  if (childConfirmations > 0) {
    await waitForBtcTransaction(logger, txids.childTxid, childConfirmations, client);
  }

  return txids;
}

/**
 * Creates a btc transactions with multiple UTXOs spending to the same address.
 *
 * @param logger - Logger to use
 * @param address - Target address of the child tx
 * @param fineAmounts - List of amounts to spend to address
 * @returns - The txids of the parent and child tx
 */
export async function sendBtcTransactionWithMultipleUtxosToSameAddress(
  address: string,
  fineAmounts: number[],
): Promise<{ txid: string }> {
  return btcClientMutex.runExclusive(async () => {
    // this is required for the `bitcoinjs` library to work.
    bitcoinjs.initEccLib(ecc);

    // construct a transaction with `bitcoinjs`. We can't use the usual bitcoin-cli,
    // because it explicitly checks that there aren't multiple outputs to the same address.
    const tx = new bitcoinjs.Transaction();
    const scriptBuffer = bitcoinjs.address.toOutputScript(address, bitcoinjs.networks.regtest);
    for (const fineAmount of fineAmounts) {
      tx.addOutput(scriptBuffer, fineAmount);
    }

    // Once we have added the outputs, all other steps (funding, signing, sending) can be done as usual with bitcoin-cli.
    const fundedTx = (await btcClient.fundRawTransaction(tx.toHex(), {
      changeAddress: await btcClient.getNewAddress(),
      feeRate: 0.00001,
      lockUnspents: true,
    })) as { hex: string };

    // Sign the tx
    const signedTx = await btcClient.signRawTransactionWithWallet(fundedTx.hex);

    // Send the tx
    const txid = (await btcClient.sendRawTransaction(signedTx.hex)) as string;

    return { txid };
  });
}
