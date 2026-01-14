import Client from 'bitcoin-core';
import * as bitcoinjs from 'bitcoinjs-lib';
import * as ecc from 'tiny-secp256k1';
import { sleep, btcClientMutex, getBtcClient } from 'shared/utils';
import { Logger, throwError } from 'shared/utils/logger';
import { ILogger } from 'shared/utils/logger_interface';
import { ChainflipIO } from './utils/chainflip_io';
import { Mutex } from 'async-mutex';

export const BTC_ENDPOINT = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';

class BtcClient {
  private readonly name: string;
  private readonly client: Client;
  private readonly mutex: Mutex;

  constructor(name: string, client: Client) {
    this.name = name;
    this.client = client;
    this.mutex = new Mutex();
  }

  async ensureFunded(logger: Logger) {
    const balance = (await this.client.getBalance()) as any as number;
    if (balance <= 100.0) {
      if (this.name === 'whale') {
        throw new Error(`The whale wallet is underfunded, current balance ${balance}`);
      }

      logger.debug(
        `The wallet ${this.name} is underfunded, current balance ${balance}. Topping up.`,
      );

      const fundingAddress = await this.client.getNewAddress();
      const hash = await sendBtc(logger, fundingAddress, 200, 1, btcClient);

      logger.debug(`Funded with 200 btc in tx ${hash}`);
    }
  }

  runExclusive<A>(f: (client: Client) => Promise<A>): Promise<A> {
    return this.mutex.runExclusive(() => f(this.client));
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
}

export const btcClient = new BtcClient(
  'whale',
  new Client({
    host: BTC_ENDPOINT.split(':')[1].slice(2),
    port: Number(BTC_ENDPOINT.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet: 'whale',
  }),
);

function getBtcClient2(wallet: string = 'watch'): Client {
  const endpoint = process.env.BTC_ENDPOINT || 'http://127.0.0.1:8332';

  return new Client({
    host: endpoint.split(':')[1].slice(2),
    port: Number(endpoint.split(':')[2]),
    username: 'flip',
    password: 'flip',
    wallet,
  });
}

const btcClients: Record<string, BtcClient> = {
  wallet1: new BtcClient('wallet1', getBtcClient2('wallet1')),
  wallet2: new BtcClient('wallet2', getBtcClient2('wallet2')),
  wallet3: new BtcClient('wallet3', getBtcClient2('wallet3')),
  wallet4: new BtcClient('wallet4', getBtcClient2('wallet4')),
  wallet5: new BtcClient('wallet5', getBtcClient2('wallet5')),
};

export async function setupAllBtcWallets<A>(cf: ChainflipIO<A>) {
  await cf.all([
    (subcf) => setupWallet(subcf, 'wallet1'),
    (subcf) => setupWallet(subcf, 'wallet2'),
    (subcf) => setupWallet(subcf, 'wallet3'),
    (subcf) => setupWallet(subcf, 'wallet4'),
    (subcf) => setupWallet(subcf, 'wallet5'),
  ]);
}

export async function getRandomBtcClient(logger: Logger): Promise<BtcClient> {
  const keys = Object.keys(btcClients);
  if (keys.length === 0) {
    throw new Error("Expected btcClients to be populated, but it wasn't. (empty object)");
  }
  const chosen = keys[Math.floor(Math.random() * keys.length)];
  const client = btcClients[chosen];
  await client.ensureFunded(logger);
  return client;
}

export async function setupWallet(logger: ILogger, name: string) {
  const newClient = await btcClient.runExclusive(async (client) => {
    const reply: any = await client.createWallet(name, false, false, '');
    if (!reply.name) {
      throw new Error(`Could not create tainted wallet, with error ${reply.warning}`);
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
  await sendBtc(logger.as_pino(), fundingAddress, 200, 1);
  logger.info(`funding success!`);

  return new BtcClient(name, newClient);
}

async function assertCanSubmitRawTx(rawTx: string, client: Client) {
  const check: any = await client.testMempoolAccept([rawTx]);
  if (!check[0].allowed) {
    throw new Error(`Bitcoin tx failed mempool accept check with '${check[0]['reject-reason']}'`);
  }
}

export async function fundAndSendTransaction(
  logger: Logger,
  outputs: object[],
  changeAddress: string,
  feeRate?: number,
  client = btcClient,
): Promise<string> {
  logger.debug(`Waiting for bitcoin mutex`);
  return client
    .runExclusive(async (c) => {
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
      logger.debug(`checking in mempool`);
      await assertCanSubmitRawTx(signedTx.hex, c);
      logger.debug(`sending`);
      const txId = (await c.sendRawTransaction(signedTx.hex)) as string | undefined;

      if (!txId) {
        throw new Error('Broadcast failed');
      }

      logger.debug(`sending done (txid: ${txId}), dropping mutex`);

      return txId;
    })
    .then((val) => {
      logger.debug(`bitcoin mutex dropped`);
      return val;
    });
}

export async function sendVaultTransaction(
  logger: Logger,
  nulldataPayload: string,
  amountBtc: number,
  depositAddress: string,
  refundAddress: string,
  client = btcClient,
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
  logger: Logger,
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
  const txids = await client.runExclusive(async (client) => {
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

    // verify
    await assertCanSubmitRawTx(childSignedTx.hex, client);

    // Send the signed tx
    const childTxid = (await client.sendRawTransaction(childSignedTx.hex)) as string;

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

    return btcClient.runExclusive(async (client) => {
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
