import Client from 'bitcoin-core';
import * as bitcoinjs from 'bitcoinjs-lib';
import * as ecc from 'tiny-secp256k1';
import { sleep, btcClientMutex, getBtcClient } from 'shared/utils';
import { ILogger } from 'shared/utils/logger_interface';
import { TrackedMutex } from 'shared/utils/tracked_mutex';
import { ChainflipIO } from 'shared/utils/chainflip_io';

export const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';

class BtcMutexClient {
  private readonly name: string;

  private readonly client: Client;

  private readonly mutex: TrackedMutex;

  constructor(name: string, client: Client) {
    this.name = name;
    this.client = client;
    this.mutex = new TrackedMutex(`BtcMutexClient(${name})`);
  }

  runExclusive<A>(logger: ILogger, f: (client: Client) => Promise<A>): Promise<A> {
    return this.mutex.runExclusive(async () => {
      await this.ensureFunded(logger);
      return f(this.client);
    });
  }

  getTransaction(id: string) {
    return this.client.getTransaction(id);
  }

  getNewAddress() {
    return this.client.getNewAddress();
  }

  unsafe_getClient(): Client {
    return this.client;
  }

  private async ensureFunded(logger: ILogger) {
    const balance = (await this.client.getBalance()) as number;
    if (balance <= 100.0) {
      if (this.name === 'whale') {
        throw new Error(`The whale wallet is underfunded, current balance ${balance}`);
      }

      logger.debug(
        `The wallet ${this.name} is underfunded, current balance ${balance}. Topping up.`,
      );

      const fundingAddress = await this.client.getNewAddress();
      // eslint-disable-next-line @typescript-eslint/no-use-before-define
      const hash = await sendBtc(logger, fundingAddress, 200, 1);

      logger.debug(`Funded with 200 btc in tx ${hash}`);
    }
  }
}

export const globalBtcWhaleMutexClient = new BtcMutexClient(
  'whale',
  new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'whale',
  }),
);

const btcClients: Record<string, BtcMutexClient> = {
  wallet1: new BtcMutexClient('wallet1', getBtcClient('wallet1')),
  wallet2: new BtcMutexClient('wallet2', getBtcClient('wallet2')),
  wallet3: new BtcMutexClient('wallet3', getBtcClient('wallet3')),
  wallet4: new BtcMutexClient('wallet4', getBtcClient('wallet4')),
  wallet5: new BtcMutexClient('wallet5', getBtcClient('wallet5')),
  wallet6: new BtcMutexClient('wallet6', getBtcClient('wallet6')),
  wallet7: new BtcMutexClient('wallet7', getBtcClient('wallet7')),
  wallet8: new BtcMutexClient('wallet8', getBtcClient('wallet8')),
  wallet9: new BtcMutexClient('wallet9', getBtcClient('wallet9')),
};

async function assertCanSubmitRawTx(rawTx: string, client: Client) {
  const check = (await client.testMempoolAccept([rawTx])) as {
    allowed: boolean;
    'reject-reason'?: string;
  }[];
  if (!check[0].allowed) {
    throw new Error(`Bitcoin tx failed mempool accept check with '${check[0]['reject-reason']}'`);
  }
}

export async function fundAndSendTransaction(
  logger: ILogger,
  outputs: object[],
  changeAddress: string,
  feeRate?: number,
  client = globalBtcWhaleMutexClient,
): Promise<string> {
  logger.debug(`Waiting for bitcoin mutex`);
  return client.runExclusive(logger, async (c) => {
    logger.debug(`Acquired mutex, creating raw tx`);
    const rawTx = await c.createRawTransaction([], outputs);
    logger.debug(`funding raw tx`);
    const fundedTx = (await c.fundRawTransaction(rawTx, {
      changeAddress,
      feeRate: feeRate ?? 0.00001,
      lockUnspents: true,
      changePosition: outputs.length,
    })) as { hex: string };
    logger.debug(`signing raw tx`);
    const signedTx = await c.signRawTransactionWithWallet(fundedTx.hex);
    logger.debug(`checking that signed raw tx could be submitted`);
    await assertCanSubmitRawTx(signedTx.hex, c);
    logger.debug(`sending`);
    const txId = (await c.sendRawTransaction(signedTx.hex)) as string | undefined;

    if (!txId) {
      throw new Error('Broadcast failed');
    }

    return txId;
  });
}

export async function sendVaultTransaction(
  logger: ILogger,
  nulldataPayload: string,
  amountBtc: number,
  depositAddress: string,
  refundAddress: string,
  client = globalBtcWhaleMutexClient,
): Promise<string> {
  return fundAndSendTransaction(
    logger,
    [
      {
        [depositAddress]: amountBtc,
      },
      {
        data: nulldataPayload.replace('0x', ''),
      },
    ],
    refundAddress,
    undefined,
    client,
  );
}

async function waitForBtcTransaction(
  logger: ILogger,
  txid: string,
  confirmations: number,
  client: Client,
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
  throw new Error(
    `Timeout (${timeoutSeconds}s) waiting for Btc transaction to be confirmed, txid: ${txid}`,
  );
}

export async function sendBtc(
  logger: ILogger,
  address: string,
  amount: number | string,
  confirmations = 1,
  client = globalBtcWhaleMutexClient,
): Promise<string> {
  // Btc client has a limit on the number of concurrent requests
  let txid: string;
  let attempts = 0;
  const maxAttempts = 3;

  // The client will error if the amount has more than 8 decimal places
  const roundedAmount = Math.round(Number(amount) * 1e8) / 1e8;

  while (attempts < maxAttempts) {
    try {
      logger.debug(`Sending ${roundedAmount}btc to ${address}.`);
      txid = await fundAndSendTransaction(
        logger,
        [
          {
            [address]: roundedAmount,
          },
        ],
        await client.getNewAddress(),
        undefined,
        client,
      );
      logger.debug(`Transaction has txhash ${txid}.`);

      if (confirmations > 0) {
        await waitForBtcTransaction(logger, txid, confirmations, client.unsafe_getClient());
        logger.debug(`Transaction confirmed with ${confirmations} confirmations`);
      } else {
        logger.debug(`Not waiting for confirmation`);
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

export async function setupWallet(logger: ILogger, name: string) {
  const newClient = await globalBtcWhaleMutexClient.runExclusive(logger, async (client) => {
    const reply = (await client.createWallet(name, false, false, '')) as {
      name?: string;
      warning?: string;
    };
    if (!reply.name) {
      throw new Error(`Could not create btc wallet with name ${name}, with error ${reply.warning}`);
    }
    if (reply.name !== name) {
      throw new Error(
        `Expected btc wallet to be created with name ${name}, but got name ${reply.name}`,
      );
    }
    logger.info(`Created new wallet: ${reply.name}`);
    return getBtcClient(reply.name);
  });
  const fundingAddress = await newClient.getNewAddress();
  logger.info(`funding wallet with 200btc to ${fundingAddress}`);
  await sendBtc(logger, fundingAddress, 200, 1);
  logger.info(`funding success!`);

  return new BtcMutexClient(name, newClient);
}

export async function setupAllBtcWallets<A>(cf: ChainflipIO<A>) {
  await cf.all([
    (subcf) => setupWallet(subcf, 'wallet1'),
    (subcf) => setupWallet(subcf, 'wallet2'),
    (subcf) => setupWallet(subcf, 'wallet3'),
    (subcf) => setupWallet(subcf, 'wallet4'),
    (subcf) => setupWallet(subcf, 'wallet5'),
    (subcf) => setupWallet(subcf, 'wallet6'),
    (subcf) => setupWallet(subcf, 'wallet7'),
    (subcf) => setupWallet(subcf, 'wallet8'),
    (subcf) => setupWallet(subcf, 'wallet9'),
  ]);
}

export async function getRandomBtcClient(_logger: ILogger): Promise<BtcMutexClient> {
  const keys = Object.keys(btcClients);
  if (keys.length === 0) {
    throw new Error("Expected btcClients to be populated, but it wasn't. (empty object)");
  }
  const chosen = keys[Math.floor(Math.random() * keys.length)];
  return btcClients[chosen];
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
  logger: ILogger,
  address: string,
  amount: number,
  parentConfirmations: number,
  childConfirmations: number,
  client = globalBtcWhaleMutexClient,
): Promise<{ parentTxid: string; childTxid: string }> {
  // Btc client has a limit on the number of concurrent requests
  const txids = await client.runExclusive(logger, async (c) => {
    // create a new address in our wallet that we have the keys for
    const intermediateAddress = await c.getNewAddress();

    // amount to use for the parent tx
    // Note: bitcoin has 8 decimal places
    const parentAmount = (amount * 1.1).toFixed(8);

    // send the parent tx
    const parentTxid = (await c.sendToAddress(
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
      await waitForBtcTransaction(logger, parentTxid, parentConfirmations, c);
    }

    // Create a raw transaction for the child tx
    const childRawTx = await c.createRawTransaction(
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
    const childFundedTx = (await c.fundRawTransaction(childRawTx, {
      changeAddress: await c.getNewAddress(),
      feeRate: 0.00001,
      lockUnspents: true,
    })) as { hex: string };

    // Sign the child tx
    const childSignedTx = await c.signRawTransactionWithWallet(childFundedTx.hex);

    // verify
    await assertCanSubmitRawTx(childSignedTx.hex, c);

    // Send the signed tx
    const childTxid = (await c.sendRawTransaction(childSignedTx.hex)) as string;

    return { parentTxid, childTxid };
  });

  if (childConfirmations > 0) {
    await waitForBtcTransaction(
      logger,
      txids.childTxid,
      childConfirmations,
      client.unsafe_getClient(),
    );
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
  logger: ILogger,
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

    return globalBtcWhaleMutexClient.runExclusive(logger, async (client) => {
      // Once we have added the outputs, all other steps (funding, signing, sending) can be done as usual with bitcoin-cli.
      const fundedTx = (await client.fundRawTransaction(tx.toHex(), {
        changeAddress: await client.getNewAddress(),
        feeRate: 0.00001,
        lockUnspents: true,
      })) as { hex: string };

      // Sign the tx
      const signedTx = await client.signRawTransactionWithWallet(fundedTx.hex);

      // verify
      await assertCanSubmitRawTx(signedTx.hex, client);

      // Send the tx
      const txid = (await client.sendRawTransaction(signedTx.hex)) as string;

      return { txid };
    });
  });
}
