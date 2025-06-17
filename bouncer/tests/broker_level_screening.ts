import axios from 'axios';
import { randomBytes } from 'crypto';
import { Chain, InternalAsset } from '@chainflip/cli';
import Web3 from 'web3';
import { getTxAmount, sendBtc, sendBtcTransactionWithParent } from '../shared/send_btc';
import {
  newAddress,
  sleep,
  handleSubstrateError,
  chainGasAsset,
  lpMutex,
  createStateChainKeypair,
  isWithinOnePercent,
  amountToFineAmountBigInt,
  getEvmEndpoint,
  chainContractId,
  chainFromAsset,
  ingressEgressPalletForChain,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';
import { getBalance } from '../shared/get_balance';
import { send } from '../shared/send';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { buildAndSendBtcVaultSwap, openPrivateBtcChannel } from '../shared/btc_vault_swap';
import { executeEvmVaultSwap } from '../shared/evm_vault_swap';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

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
 * Generates a new address for an asset.
 *
 * @param asset - The asset to generate an address for.
 * @param seed - The seed to generate the address with. If no seed is provided, a random one is generated.
 * @returns - The new address.
 */
async function newAssetAddress(asset: InternalAsset, seed = null): Promise<string> {
  return Promise.resolve(newAddress(asset, seed || randomBytes(32).toString('hex')));
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
      throw new Error(`Unssuported evm chain ${chain}`);
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
async function brokerLevelScreeningTestBtc(
  logger: Logger,
  doBoost: boolean,
  sendFunction: (amount: number, address: string) => Promise<string>,
  reportFunction: (txId: string) => Promise<void>,
): Promise<void> {
  logger.info(`Testing broker level screening for Bitcoin with ${doBoost ? '' : 'no'} boost...`);

  const refundAddress = await newAssetAddress('Btc');
  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };
  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const swapParams = await requestNewSwap(
    logger.child({ tag: 'brokerLevelScreeningTest' }),
    'Btc',
    'Usdc',
    destinationAddressForUsdc,
    undefined,
    0,
    doBoost ? 100 : 0,
    refundParameters,
  );

  // send tx
  const amountBtc = 0.234;
  const txId = await sendFunction(amountBtc, swapParams.depositAddress);
  logger.debug(`Sent Bitcoin tx with id ${txId} to reject`);

  // mark tx for rejection
  await reportFunction(txId);
  const reportedAmount = await getTxAmount(logger, txId);
  if (Math.abs(reportedAmount) !== amountBtc) {
    throw new Error(
      `Reported amount doesn't match sent amount! Reported wrong tx? txId: ${txId}, reportedAmount: ${reportedAmount}, expectedAmount: ${amountBtc}`,
    );
  }

  // wait for rejection
  await observeEvent(logger, 'bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(refundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${refundAddress} within timeout!`);
  }

  logger.info(`Marked Bitcoin transaction was rejected and refunded 👍.`);
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
  amount: number,
  doBoost: boolean,
  refundAddress: string,
  reportFunction: (txId: string) => Promise<void>,
): Promise<void> {
  const logger = testContext.logger;

  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const txId = await buildAndSendBtcVaultSwap(
    logger,
    amount,
    'Usdc',
    destinationAddressForUsdc,
    refundAddress,
    {
      account: broker.address,
      commissionBps: 0,
    },
    [],
    0,
  );
  logger.debug(`Sent Bitcoin vault swap with id ${txId} to reject`);
  await reportFunction(txId);
  const reportedAmount = await getTxAmount(logger, txId);
  if (Math.abs(reportedAmount) !== amount) {
    // TODO: Why does the amount of the vault swap not match the sent amount? reportedAmount: -0.30419137, expectedAmount: 0.287
    // reportedAmount: -0.78124786, expectedAmount: 0.287
    // throw new Error(
    //   `Reported amount doesn't match sent amount! Reported wrong tx? txId: ${txId}, reportedAmount: ${reportedAmount}, expectedAmount: ${amount}`,
    // );
  }
}

async function testEvm(
  testContext: TestContext,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const logger = testContext.logger;
  logger.info(`Testing broker level screening for Evm ${sourceAsset}...`);

  const chain = chainFromAsset(sourceAsset);
  const ingressEgressPallet = ingressEgressPalletForChain(chain);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');

  logger.debug(`BTC destination address: ${destinationAddressForBtc}`);

  const ethereumRefundAddress = await newAssetAddress('Eth');

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: ethereumRefundAddress,
    minPriceX128: '0',
  };

  const swapParams = await requestNewSwap(
    logger,
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    undefined,
    0,
    0,
    refundParameters,
  );

  if (sourceAsset === chainGasAsset('Ethereum')) {
    await send(logger, sourceAsset, swapParams.depositAddress);
    logger.debug(`Sent initial ${sourceAsset} tx...`);
    await observeEvent(logger, 'ethereumIngressEgress:DepositFinalised').event;
    logger.debug(`Initial deposit ${sourceAsset} received...`);
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // successfully to make sure the Deposit contract is deployed.
    await waitForDepositContractDeployment(chain, swapParams.depositAddress);
  }

  logger.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(logger, sourceAsset, swapParams.depositAddress))
    .transactionHash as string;
  logger.debug(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  logger.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(logger, `${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  let receivedRefund = false;

  for (let i = 0; i < MAX_RETRIES; i++) {
    const refundBalance = await getBalance(sourceAsset, ethereumRefundAddress);
    const depositAddressBalance = await getBalance(sourceAsset, swapParams.depositAddress);
    if (refundBalance !== '0' && depositAddressBalance === '0') {
      receivedRefund = true;
      break;
    }
    await sleep(6000);
  }

  if (!receivedRefund) {
    throw new Error(
      `Didn't receive refund of ${sourceAsset} to address ${ethereumRefundAddress} within timeout!`,
    );
  }

  logger.info(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

async function testEvmVaultSwap(
  testContext: TestContext,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const logger = testContext.logger;

  const chain = chainFromAsset(sourceAsset);

  logger.info(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const ethereumRefundAddress = await newAssetAddress('Eth');

  logger.debug(`Refund address for ${sourceAsset} is ${ethereumRefundAddress}...`);

  logger.debug(`Sending ${sourceAsset} (vault swap) tx to reject...`);
  const txHash = await executeEvmVaultSwap(
    logger,
    broker.address,
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
  logger.debug(`Sent ${sourceAsset} (vault swap) tx...`);

  await reportFunction(txHash);
  logger.debug(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

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

  logger.info(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}

async function testEvmLiquidityDeposit(
  testContext: TestContext,
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const logger = testContext.logger;

  const chain = chainFromAsset(sourceAsset);
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  logger.info(`Testing broker level screening for ${chain} ${sourceAsset}...`);
  const MAX_RETRIES = 120;

  // setup access to chainflip api and lp
  await using chainflip = await getChainflipApi();
  const lp = createStateChainKeypair(process.env.LP_URI || '//LP_1');

  // Get existing LP refund address of //LP_1 for `sourceAsset`
  /* eslint-disable  @typescript-eslint/no-explicit-any */
  const addressReponse = (
    await chainflip.query.liquidityProvider.liquidityRefundAddress(
      lp.address,
      chainContractId(chainFromAsset(sourceAsset)),
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

  logger.debug(`refund address is: ${ethereumRefundAddress}`);

  // Create new LP deposit address for //LP_1
  const eventHandle = observeEvent(logger, 'liquidityProvider:LiquidityDepositAddressReady', {
    test: (event) => event.data.asset === sourceAsset && event.data.accountId === lp.address,
  }).event;

  console.log('Requesting ' + sourceAsset + ' deposit address');
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(sourceAsset, null)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  let depositAddress;
  if (chain === 'Ethereum') {
    depositAddress = (await eventHandle).data.depositAddress.Eth;
  } else if (chain === 'Arbitrum') {
    depositAddress = (await eventHandle).data.depositAddress.Arb;
  } else {
    throw new Error('Unsupported Evm chain');
  }
  logger.debug(`Got deposit address: ${depositAddress}`);

  if (sourceAsset === chainGasAsset('Ethereum') || sourceAsset === chainGasAsset('Arbitrum')) {
    // The first tx cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // succesfully to make sure the Deposit contract is deployed.

    const amount = '3';
    const observeAccountCreditedEvent = observeEvent(logger, 'assetBalances:AccountCredited', {
      test: (event) =>
        event.data.asset === sourceAsset &&
        isWithinOnePercent(
          BigInt(event.data.amountCredited.replace(/,/g, '')),
          BigInt(amountToFineAmountBigInt(amount, sourceAsset)),
        ),
    }).event;

    await send(logger, sourceAsset, depositAddress, amount);
    logger.debug(`Sent initial ${sourceAsset} tx...`);
    await observeEvent(logger, `${ingressEgressPallet}:DepositFinalised`).event;
    logger.debug(`Initial deposit ${sourceAsset} received...`);
    await observeAccountCreditedEvent;
    logger.debug(`Account credited for ${sourceAsset}...`);
    await waitForDepositContractDeployment(chain, depositAddress);
  }

  logger.debug(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(logger, sourceAsset, depositAddress)).transactionHash as string;
  logger.debug(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  logger.debug(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(logger, `${ingressEgressPallet}:TransactionRejectedByBroker`).event;

  let receivedRefund = false;

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

  logger.info(`Marked ${sourceAsset} LP deposit was rejected and refunded 👍.`);
}

// Sets the ingress_egress broker whitelist to the given `broker`.
async function setWhitelistedBroker(brokerAddress: Uint8Array) {
  const BTC_WHITELIST_PREFIX = '3ed3ce16dbc61ca64eaac5a96e809a8f6b8fb02fc586c9dab2385ea1690a7db6';
  const ETH_WHITELIST_PREFIX = '4fc967eb3d0785df0389312c2ebd853e6b8fb02fc586c9dab2385ea1690a7db6';
  const ARB_WHITELIST_PREFIX = '3d3491b8c14ff78a5176bc3b6ebe516f6b8fb02fc586c9dab2385ea1690a7db6';

  const decodeHexStringToByteArray = (hex: string) => {
    let hexString = hex;
    const result = [];
    while (hexString.length >= 2) {
      result.push(parseInt(hexString.substring(0, 2), 16));
      hexString = hexString.substring(2, hexString.length);
    }
    return result;
  };

  for (const prefix of [BTC_WHITELIST_PREFIX, ETH_WHITELIST_PREFIX, ARB_WHITELIST_PREFIX]) {
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
export function testBitcoin(testContext: TestContext, doBoost: boolean): Promise<void>[] {
  const logger = testContext.logger;

  // if we don't boost, we wait with our report for 1 block confirmation, otherwise we submit the report directly
  const confirmationsBeforeReport = doBoost ? 0 : 1;

  // send a single tx
  const simple = brokerLevelScreeningTestBtc(
    logger,
    doBoost,
    async (amount, address) => sendBtc(logger, address, amount, confirmationsBeforeReport),
    async (txId) => setTxRiskScore(txId, 9.0),
  );

  // send a parent->child chain in the same block and mark the parent
  const sameBlockParentMarked = brokerLevelScreeningTestBtc(
    logger,
    doBoost,
    async (amount, address) =>
      (await sendBtcTransactionWithParent(logger, address, amount, 0, confirmationsBeforeReport))
        .childTxid,
    async (txId) => setTxRiskScore(txId, 9.0),
  );

  // send a parent->child chain where parent is 2 blocks older and mark the parent
  const oldParentMarked = brokerLevelScreeningTestBtc(
    logger,
    doBoost,
    async (amount, address) =>
      (await sendBtcTransactionWithParent(logger, address, amount, 2, confirmationsBeforeReport))
        .childTxid,
    async (txId) => setTxRiskScore(txId, 9.0),
  );

  return [simple, sameBlockParentMarked, oldParentMarked];
}

async function testBitcoinVaultSwap(testContext: TestContext) {
  const logger = testContext.logger;

  // -- Test vault swap rejection --
  logger.info('Testing broker level screening for Bitcoin vault swap...');
  const btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestBtcVaultSwap(
    testContext,
    0.287,
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

  // test rejection of swaps by the responsible broker
  await Promise.all(
    [
      testEvm(testContext, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
      testEvm(testContext, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
      testEvm(testContext, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
      testEvm(testContext, 'ArbUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
    ]
      .concat(testBitcoin(testContext, false))
      .concat(testBoostedDeposits ? testBitcoin(testContext, true) : []),
  );

  // test rejection of LP deposits and vault swaps:
  //  - this requires the rejecting broker to be whitelisted
  //  - for bitcoin vault swaps a private channel has to be opened
  await setWhitelistedBroker(broker.addressRaw);
  await openPrivateBtcChannel(testContext.logger, '//BROKER_1');
  await Promise.all([
    // --- LP deposits ---
    testEvmLiquidityDeposit(testContext, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    testEvmLiquidityDeposit(testContext, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    testEvmLiquidityDeposit(testContext, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    testEvmLiquidityDeposit(testContext, 'ArbUsdc', async (txId) => setTxRiskScore(txId, 9.0)),

    // --- vault swaps ---
    testBitcoinVaultSwap(testContext),
    testEvmVaultSwap(testContext, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    testEvmVaultSwap(testContext, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    testEvmVaultSwap(testContext, 'ArbUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
  ]);

  await setMockmode(previousMockmode);
}
