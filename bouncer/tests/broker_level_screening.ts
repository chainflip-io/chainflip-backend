import axios from 'axios';
import { Chain, InternalAsset } from '@chainflip/cli';
import Web3 from 'web3';
import { btcClient, sendBtc, sendBtcTransactionWithParent } from 'shared/send_btc';
import {
  newAssetAddress,
  sleep,
  chainGasAsset,
  isWithinOnePercent,
  amountToFineAmountBigInt,
  getEvmEndpoint,
  chainFromAsset,
  ingressEgressPalletForChain,
  observeBalanceIncrease,
  observeCcmReceived,
  observeFetch,
  btcClientMutex,
  getBtcClient,
  getChainContractId,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { requestNewSwap } from 'shared/perform_swap';
import { FillOrKillParamsX128 } from 'shared/new_swap';
import { getBtcBalance } from 'shared/get_btc_balance';
import { TestContext } from 'shared/utils/test_context';
import { getIsoTime, globalLogger } from 'shared/utils/logger';
import { getBalance } from 'shared/get_balance';
import { send } from 'shared/send';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { buildAndSendBtcVaultSwap } from 'shared/btc_vault_swap';
import { executeEvmVaultSwap } from 'shared/evm_vault_swap';
import { newCcmMetadata } from 'shared/swapping';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { testSol, testSolVaultSwap } from 'tests/broker_level_screening/sol';
import { bitcoinIngressEgressTransactionRejectedByBroker } from 'generated/events/bitcoinIngressEgress/transactionRejectedByBroker';
import { ethereumIngressEgressDepositFinalised } from 'generated/events/ethereumIngressEgress/depositFinalised';
import { ethereumIngressEgressTransactionRejectedByBroker } from 'generated/events/ethereumIngressEgress/transactionRejectedByBroker';
import { liquidityProviderLiquidityDepositAddressReady } from 'generated/events/liquidityProvider/liquidityDepositAddressReady';
import { assetBalancesAccountCredited } from 'generated/events/assetBalances/accountCredited';
import { capitalize } from '@chainflip/utils/string';

const brokerUri = '//BROKER_1';

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
    await sleep(1000);
    const balance = await getBtcBalance(address);
    if (balance !== initialBalance) {
      return Promise.resolve(true);
    }
  }
  console.error(`BTC balance for ${address} did not change after ${MAX_RETRIES} seconds.`);
  return Promise.resolve(false);
}

/**
 * Submit a post request to the deposit-monitor, with error handling.
 * @param portAndRoute Where we want to submit the request to.
 * @param body The request body, is serialized as JSON.
 */
async function postToDepositMonitor(portAndRoute: string, body: string | object) {
  return axios
    .post('http://127.0.0.1' + portAndRoute, JSON.stringify(body), {
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
      timeout: 5000,
    })
    .then((res) => res.data)
    .catch((error) => {
      let message;
      if (error.response) {
        message = `${error.response.data} (${error.response.status})`;
      } else {
        message = error;
      }
      throw new Error(`Request to deposit monitor (${portAndRoute}) failed: ${message}`);
    });
}

/**
 * Typescript representation of the allowed parameters to `setMockmode`. The JSON encoding of these
 * is what the deposit-monitor expects.
 */
type Mockmode =
  | 'Manual'
  | { Deterministic: { score: number; incomplete_probability: number } }
  | { Random: { min_score: number; max_score: number; incomplete_probability: number } };

/**
 * Set the mockmode of the deposit monitor, controlling how it analyses incoming transactions.
 *
 * @param mode Object describing the mockmode we want to set the deposit-monitor to,
 */
async function setMockmode(mode: Mockmode) {
  return postToDepositMonitor(':6070/mockmode', mode);
}

/**
 * Call the deposit-monitor to set risk score of given transaction in mock analysis provider.
 *
 * @param txid Hash of the transaction we want to report.
 * @param score Risk score for this transaction. Can be in range [0.0, 10.0].
 */
async function setTxRiskScore(txid: string, score: number) {
  await postToDepositMonitor(':6070/riskscore', [
    txid,
    {
      risk_score: { Score: score },
      unknown_contribution_percentage: 0.0,
    },
  ]);
}

/**
 * Checks that the deposit monitor has started up successfully and is healthy.
 */
async function ensureHealth() {
  const response = await postToDepositMonitor(':6060/health', {});
  globalLogger.info(`DM health response is: ${JSON.stringify(response)}`);
  if (response.starting === true || response.all_processors === false) {
    throw new Error(
      `Deposit monitor is running, but not healthy. It's response was: ${JSON.stringify(response)}`,
    );
  }
}

/**
 * Wait for the Deposit contract to be deployed.
 */

async function waitForDepositContractDeployment(chain: Chain, depositAddress: string) {
  switch (chain) {
    case 'Arbitrum':
    case 'Ethereum':
      break;
    default:
      throw new Error(`Unsupported evm chain ${chain}`);
  }

  const MAX_RETRIES = 100;
  const web3 = new Web3(getEvmEndpoint(chain));
  let contractDeployed = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const bytecode = await web3.eth.getCode(depositAddress);
    if (bytecode && bytecode !== '0x') {
      contractDeployed = true;
      break;
    }
    await sleep(6000);
  }
  if (!contractDeployed) {
    throw new Error(`Ethereum contract not deployed at address ${depositAddress} within timeout!`);
  }
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @returns - The the channel id of the deposit channel.
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
  await cf.stepUntilEvent(
    'BitcoinIngressEgress.TransactionRejectedByBroker',
    bitcoinIngressEgressTransactionRejectedByBroker,
  );
  if (!(await observeBtcAddressBalanceChange(refundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${refundAddress} within timeout!`);
  }

  cf.info(`Marked Bitcoin transaction was rejected and refunded 👍.`);
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @returns - The the channel id of the deposit channel.
 */
async function brokerLevelScreeningTestBtcVaultSwap(
  testContext: TestContext,
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  reportFunction: (txId: string) => Promise<void>,
): Promise<void> {
  const logger = testContext.logger;

  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const txId = await buildAndSendBtcVaultSwap(
    logger,
    brokerUri,
    parseFloat(amount),
    'Usdc',
    destinationAddressForUsdc,
    refundAddress,
    0,
    [],
  );
  await reportFunction(txId);
}

async function testEvm<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
  ccmRefund = false,
) {
  cf.info(`Testing broker level screening for Evm ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);

  const destinationAddressForBtc = await newAssetAddress('Btc');

  cf.debug(`BTC destination address: ${destinationAddressForBtc}`);

  const ethereumRefundAddress = await newAssetAddress('Eth', undefined, undefined, ccmRefund);
  const initialRefundAddressBalance = await getBalance(sourceAsset, ethereumRefundAddress);

  const refundCcmMetadata = ccmRefund ? await newCcmMetadata(sourceAsset) : undefined;

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: ethereumRefundAddress,
    minPriceX128: '0',
    refundCcmMetadata,
  };

  const swapParams = await requestNewSwap(
    cf,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    undefined,
    0,
    0,
    refundParameters,
  );

  if (sourceAsset === chainGasAsset('Ethereum')) {
    await send(cf.logger, sourceAsset, swapParams.depositAddress);
    cf.debug(`Sent initial ${sourceAsset} tx...`);
    await cf.stepUntilEvent(
      'EthereumIngressEgress.DepositFinalised',
      ethereumIngressEgressDepositFinalised.refine(
        (event) => event.channelId === BigInt(swapParams.channelId),
      ),
    );
    cf.debug(`Initial deposit ${sourceAsset} received...`);
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // successfully to make sure the Deposit contract is deployed.
    await waitForDepositContractDeployment(chain, swapParams.depositAddress);
  }

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(cf.logger, sourceAsset, swapParams.depositAddress))
    .transactionHash as string;
  cf.debug(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await cf.stepUntilEvent(
    `EthereumIngressEgress.TransactionRejectedByBroker`,
    ethereumIngressEgressTransactionRejectedByBroker.refine((event) =>
      event.txId.txHashes?.includes(txHash as `0x${string}`),
    ),
  );

  const ccmEventEmitted = refundParameters.refundCcmMetadata
    ? observeCcmReceived(
        sourceAsset,
        sourceAsset,
        refundParameters.refundAddress,
        refundParameters.refundCcmMetadata,
      )
    : Promise.resolve();

  await Promise.all([
    observeBalanceIncrease(
      cf.logger,
      sourceAsset,
      ethereumRefundAddress,
      initialRefundAddressBalance,
    ),
    ccmEventEmitted,
    observeFetch(sourceAsset, swapParams.depositAddress),
  ]);

  cf.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

async function testEvmVaultSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const chain = chainFromAsset(sourceAsset);

  cf.info(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const ethereumRefundAddress = await newAssetAddress('Eth');

  cf.debug(`Refund address for ${sourceAsset} is ${ethereumRefundAddress}...`);

  cf.debug(`Sending ${sourceAsset} (vault swap) tx to reject...`);
  const txHash = await executeEvmVaultSwap(
    cf.logger,
    brokerUri,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    0,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    undefined,
    [],
    ethereumRefundAddress,
  );
  cf.debug(`Sent ${sourceAsset} (vault swap) tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

  // Currently this event cannot be decoded correctly, so we don't wait for it,
  // just wait for the funds to arrive at the refund address
  // await observeEvent(`${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  let receivedRefund = false;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, ethereumRefundAddress);
    if (refundBalance !== '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive funds refund to address ${ethereumRefundAddress} within timeout!`,
    );
  }
  cf.info(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}

async function testEvmLiquidityDeposit<A = []>(
  parentcf: ChainflipIO<A>,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  // setup access to chainflip api and lp
  await using chainflip = await getChainflipApi();
  const lpUri = (process.env.LP_URI || '//LP_1') as `//${string}`;
  const cf = parentcf.with({
    account: fullAccountFromUri(lpUri, 'LP'),
  });

  const chain = chainFromAsset(sourceAsset);

  cf.info(`Testing broker level screening for ${chain} ${sourceAsset}...`);

  // Get existing LP refund address of //LP_1 for `sourceAsset`
  /* eslint-disable  @typescript-eslint/no-explicit-any */
  const addressReponse = (
    await chainflip.query.liquidityProvider.liquidityRefundAddress(
      cf.requirements.account.keypair.address,
      getChainContractId(chainFromAsset(sourceAsset)),
    )
  ).toJSON() as any;
  if (addressReponse === undefined) {
    throw new Error(`There was now refund address for ${sourceAsset} for the LP.`);
  }
  let ethereumRefundAddress;
  if (chain === 'Ethereum') {
    ethereumRefundAddress = addressReponse.eth;
  } else if (chain === 'Arbitrum') {
    ethereumRefundAddress = addressReponse.arb;
  } else {
    throw new Error('Unsupported Evm chain');
  }

  cf.debug(`refund address is: ${ethereumRefundAddress}`);

  // Create new LP deposit address for //LP_1
  const depositAddressReady = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.liquidityProvider.requestLiquidityDepositAddress(sourceAsset, null),
    expectedEvent: {
      name: 'LiquidityProvider.LiquidityDepositAddressReady',
      schema: liquidityProviderLiquidityDepositAddressReady.refine(
        (event) =>
          event.asset === sourceAsset &&
          event.accountId === cf.requirements.account.keypair.address,
      ),
    },
  });
  const depositAddress = depositAddressReady.depositAddress.address;

  cf.debug(`Got deposit address: ${depositAddress}`);

  if (sourceAsset === chainGasAsset('Ethereum') || sourceAsset === chainGasAsset('Arbitrum')) {
    // The first tx cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // succesfully to make sure the Deposit contract is deployed.

    const amount = '3';

    await send(cf.logger, sourceAsset, depositAddress, amount);
    cf.debug(`Sent initial ${sourceAsset} tx...`);

    await cf.stepUntilEvent(
      'AssetBalances.AccountCredited',
      assetBalancesAccountCredited.refine(
        (event) =>
          event.asset === sourceAsset &&
          isWithinOnePercent(
            event.amountCredited,
            BigInt(amountToFineAmountBigInt(amount, sourceAsset)),
          ) &&
          event.accountId === cf.requirements.account.keypair.address,
      ),
    );
    cf.debug(`Account credited for ${sourceAsset}...`);
    await waitForDepositContractDeployment(chain, depositAddress);
    cf.debug(`Contract deployed`);
  }

  cf.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(cf.logger, sourceAsset, depositAddress)).transactionHash as string;
  cf.debug(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  cf.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await cf.stepUntilEvent(
    `${capitalize(ingressEgressPalletForChain(chain))}.TransactionRejectedByBroker`,
    ethereumIngressEgressTransactionRejectedByBroker.refine((event) =>
      event.txId.txHashes?.includes(txHash as `0x${string}`),
    ),
  );

  let receivedRefund = false;

  const MAX_RETRIES = 120;
  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, ethereumRefundAddress);
    const depositAddressBalance = await getBalance(sourceAsset, depositAddress);
    if (refundBalance !== '0' && depositAddressBalance === '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive funds liquidity deposit refund to ${ethereumRefundAddress} within timeout!`,
    );
  }

  cf.info(`Marked ${sourceAsset} LP deposit was rejected and refunded 👍.`);
}

// Sets the ingress_egress broker whitelist to the given `broker`.
async function setWhitelistedBroker(brokerAddress: Uint8Array) {
  const BTC_WHITELIST_PREFIX = '3ed3ce16dbc61ca64eaac5a96e809a8f6b8fb02fc586c9dab2385ea1690a7db6';
  const ETH_WHITELIST_PREFIX = '4fc967eb3d0785df0389312c2ebd853e6b8fb02fc586c9dab2385ea1690a7db6';
  const ARB_WHITELIST_PREFIX = '3d3491b8c14ff78a5176bc3b6ebe516f6b8fb02fc586c9dab2385ea1690a7db6';
  const SOL_WHITELIST_PREFIX = '8595efe3a571f61007e89f4416b858b16b8fb02fc586c9dab2385ea1690a7db6';

  const decodeHexStringToByteArray = (hex: string) => {
    let hexString = hex;
    const result = [];
    while (hexString.length >= 2) {
      result.push(parseInt(hexString.substring(0, 2), 16));
      hexString = hexString.substring(2, hexString.length);
    }
    return result;
  };

  for (const prefix of [
    BTC_WHITELIST_PREFIX,
    ETH_WHITELIST_PREFIX,
    ARB_WHITELIST_PREFIX,
    SOL_WHITELIST_PREFIX,
  ]) {
    await submitGovernanceExtrinsic((api) =>
      api.tx.governance.callAsSudo(
        api.tx.system.setStorage([
          [
            decodeHexStringToByteArray(prefix).concat(Array.from(brokerAddress)),
            // Empty, we just need to insert the key.
            '',
          ],
        ]),
      ),
    );
  }
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
  // ): Promise<((cf: ChainflipIO<A>) => Promise<void>)[]> {
) {
  // we have to setup a separate wallet in order to not taint our main wallet, otherwise
  // the deposit monitor will possibly reject transactions created by other tests, due
  // to ancestor screening. This has been a source of bouncer flakiness in the past.
  const taintedClient = await btcClientMutex.runExclusive(async () => {
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
      async (txId) => setTxRiskScore(txId, 9.0),
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
      async (txId) => setTxRiskScore(txId, 9.0),
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
      async (txId) => setTxRiskScore(txId, 9.0),
    );

  return [simple, sameBlockParentMarked, oldParentMarked];
}

/* eslint-disable  @typescript-eslint/no-unused-vars */
async function testBitcoinVaultSwap(testContext: TestContext) {
  const logger = testContext.logger;

  // -- Test vault swap rejection --
  logger.info('Testing broker level screening for Bitcoin vault swap...');
  const btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestBtcVaultSwap(
    testContext,
    '0.2',
    false,
    btcRefundAddress,
    async (txId) => setTxRiskScore(txId, 9.0),
  );

  // Currently this event cannot be decoded correctly, so we don't wait for it,
  // just wait for the funds to arrive at the refund address
  // await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  logger.info(`Bitcoin vault swap was rejected and refunded 👍.`);
}

export async function testBrokerLevelScreening(
  testContext: TestContext,
  testBoostedDeposits: boolean = false,
) {
  const parentcf = await newChainflipIO(testContext.logger, []);

  await ensureHealth();
  const previousMockmode = (await setMockmode('Manual')).previous;

  // NOTE: We currently don't test the following assets:
  // - Flip: we don't test Flip rejections because they are currently disabled in the
  //         deposit monitor, since Elliptic doesn't provide Flip analysis.
  // - ArbEth: we don't test ArbEth rejections since on localnet the safety margin for ArbEth
  //           is too short for the DM, the rejections fail more often than not due
  //           to being too late.
  //           Most of the functionality is covered by testing `Eth` and `ArbUsdc`.
  //           An alternative would be to increase the ArbEth safety margin on localnet.
  // - ArbUsdc: we also don't test ArbUsdc rejections, they have caused tests to become flaky
  //            as well (PRO-2488).
  // - Btc VaultSwaps: For bitcoin, due to ancestor screening, we have to make sure to use
  //                   a dedicated "tainted" wallet. Since it's somewhat difficult to inject
  //                   a different wallet into the `sendVaultSwap` flow, we disable the test for now.

  // test rejection of swaps by the responsible broker
  await parentcf.all([
    (cf) => testSol(cf, 'Sol', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testSol(cf, 'SolUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testEvm(cf, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testEvm(cf, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testEvm(cf, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    ...(await testBitcoin(parentcf, false)),
    ...(testBoostedDeposits ? await testBitcoin(parentcf, true) : []),
  ]);

  // test rejection of LP deposits and vault swaps:
  //  - this requires the rejecting broker to be whitelisted
  //  - for bitcoin vault swaps a private channel has to be opened
  await setWhitelistedBroker(fullAccountFromUri('//BROKER_API', 'Broker').keypair.addressRaw);
  await parentcf.all([
    // --- LP deposits ---
    (cf) => testEvmLiquidityDeposit(cf, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testEvmLiquidityDeposit(cf, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testEvmLiquidityDeposit(cf, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),

    // --- vault swaps ---
    // testBitcoinVaultSwap(testContext),
    (cf) => testEvmVaultSwap(cf, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testEvmVaultSwap(cf, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testSolVaultSwap(cf, 'Sol', async (txId) => setTxRiskScore(txId, 9.0)),
    (cf) => testSolVaultSwap(cf, 'SolUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
  ]);

  await setMockmode(previousMockmode);
}
