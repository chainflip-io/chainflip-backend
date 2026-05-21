import { btcClient, sendBtc, sendBtcTransactionWithParent } from 'shared/send_btc';
import { btcClientMutex, newAssetAddress, sleep, getBtcClient } from 'shared/utils';
import { observeEvent } from 'shared/utils/substrate';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBtcBalance } from 'shared/get_btc_balance';
import { getIsoTime } from 'shared/utils/logger';
import { buildAndSendBtcVaultSwap } from 'shared/vault_swap/btc_vault_swap';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';

/**
 * Observes the balance of a BTC address and returns true if the balance changes. Times out after 100 seconds and returns false if the balance does not change.
 *
 * @param address - The address to observe the balance of.
 * @returns - Whether the balance changed.
 */
async function observeBtcAddressBalanceChange(address: string): Promise<boolean> {
  const MAX_RETRIES = 100;
  const initialBalance = await getBtcBalance(address);
  for (let i = 0; i < MAX_RETRIES; i++) {
    await sleep(3000);
    const balance = await getBtcBalance(address);
    if (balance !== initialBalance) {
      return Promise.resolve(true);
    }
  }
  console.error(`BTC balance for ${address} did not change after ${MAX_RETRIES} seconds.`);
  return Promise.resolve(false);
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param cf - The ChainflipIO instance.
 * @param doBoost - Whether to boost the deposit.
 * @param sendFunction - Function to send the BTC transaction.
 * @param reportFunction - Function to report the transaction for rejection.
 */
async function brokerLevelScreeningTestBtc<A = []>(
  cf: ChainflipIO<A>,
  doBoost: boolean,
  sendFunction: (amount: number, address: string) => Promise<string>,
  reportFunction: (txId: string) => Promise<void>,
): Promise<void> {
  cf.info(`Testing broker level screening for Bitcoin with ${doBoost ? '' : 'no'} boost...`);

  const refundAddress = await newAssetAddress('Btc');
  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };
  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const swapParams = await requestNewSwap(
    cf,
    'Btc',
    'Usdc',
    destinationAddressForUsdc,
    undefined,
    0,
    doBoost ? 100 : 0,
    refundParameters,
  );

  // send tx
  const txId = await sendFunction(0.2, swapParams.depositAddress);

  // mark tx for rejection
  await reportFunction(txId);

  // wait for rejection
  await observeEvent(cf.logger, 'bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(refundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${refundAddress} within timeout!`);
  }

  cf.info(`Marked Bitcoin transaction was rejected and refunded 👍.`);
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param cf - The ChainflipIO instance.
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @param reportFunction - Function to report the transaction for rejection.
 */
async function brokerLevelScreeningTestBtcVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  reportFunction: (txId: string) => Promise<void>,
): Promise<void> {
  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const txId = await buildAndSendBtcVaultSwap(
    cf,
    parseFloat(amount),
    'Usdc',
    destinationAddressForUsdc,
    refundAddress,
    0,
    [],
  );
  await reportFunction(txId);
}

// -- Test suite for broker level screening --
//
// In this tests we are interested in the following scenarios:
//
// 1. No boost and early tx report -> tx is reported early and the swap is refunded.
// 2. Boost and early tx report -> tx is reported early and the swap is refunded.
// 3. Boost and late tx report -> tx is reported late and the swap is not refunded.
export async function testBitcoin<A = []>(
  cf: ChainflipIO<A>,
  doBoost: boolean,
  reportFunction: (txId: string) => Promise<void>,
  // ): Promise<((cf: ChainflipIO<A>) => Promise<void>)[]> {
) {
  // we have to setup a separate wallet in order to not taint our main wallet, otherwise
  // the deposit monitor will possibly reject transactions created by other tests, due
  // to ancestor screening. This has been a source of bouncer flakiness in the past.
  const taintedClient = await btcClientMutex.runExclusive(async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const reply: any = await btcClient.createWallet(`tainted-${getIsoTime()}`, false, false, '');
    if (!reply.name) {
      throw new Error(`Could not create tainted wallet, with error ${reply.warning}`);
    }
    cf.debug(`got new wallet for BLS test: ${reply.name}`);
    return getBtcClient(reply.name);
  });
  const fundingAddress = await taintedClient.getNewAddress();
  cf.debug(`funding tainted wallet with 5btc to ${fundingAddress}`);
  await sendBtc(cf.logger, fundingAddress, 5, 1);
  cf.debug(`funding success!`);

  // if we don't boost, we wait with our report for 1 block confirmation, otherwise we submit the report directly
  const confirmationsBeforeReport = doBoost ? 0 : 1;

  // send a single tx
  const simple = (subcf: ChainflipIO<A>) =>
    brokerLevelScreeningTestBtc(
      subcf,
      doBoost,
      async (amount, address) =>
        sendBtc(subcf.logger, address, amount, confirmationsBeforeReport, taintedClient),
      reportFunction,
    );

  // send a parent->child chain in the same block and mark the parent
  const sameBlockParentMarked = (subcf: ChainflipIO<A>) =>
    brokerLevelScreeningTestBtc(
      subcf,
      doBoost,
      async (amount, address) =>
        (
          await sendBtcTransactionWithParent(
            subcf.logger,
            address,
            amount,
            0,
            confirmationsBeforeReport,
            taintedClient,
          )
        ).childTxid,
      reportFunction,
    );

  // send a parent->child chain where parent is 2 blocks older and mark the parent
  const oldParentMarked = (subcf: ChainflipIO<A>) =>
    brokerLevelScreeningTestBtc(
      subcf,
      doBoost,
      async (amount, address) =>
        (
          await sendBtcTransactionWithParent(
            subcf.logger,
            address,
            amount,
            2,
            confirmationsBeforeReport,
            taintedClient,
          )
        ).childTxid,
      reportFunction,
    );

  return [simple, sameBlockParentMarked, oldParentMarked];
}

export async function testBitcoinVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  reportFunction: (txId: string) => Promise<void>,
) {
  // -- Test vault swap rejection --
  cf.info('Testing broker level screening for Bitcoin vault swap...');
  const btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestBtcVaultSwap(cf, '0.2', false, btcRefundAddress, reportFunction);

  // Currently this event cannot be decoded correctly, so we don't wait for it,
  // just wait for the funds to arrive at the refund address
  // await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  cf.info(`Bitcoin vault swap was rejected and refunded 👍.`);
}
