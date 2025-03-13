import axios from 'axios';
import { randomBytes } from 'crypto';
import { Chain, InternalAsset } from '@chainflip/cli';
import Web3 from 'web3';
import { ExecutableTest } from '../shared/executable_test';
import { sendBtc } from '../shared/send_btc';
import {
  newAddress,
  sleep,
  handleSubstrateError,
  brokerMutex,
  chainGasAsset,
  hexStringToBytesArray,
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
import { getBalance } from '../shared/get_balance';
import { send } from '../shared/send';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { buildAndSendBtcVaultSwap, openPrivateBtcChannel } from '../shared/btc_vault_swap';
import { executeEvmVaultSwap } from '../shared/evm_vault_swap';

type SupportedChain = 'Bitcoin' | 'Ethereum' | 'Arbitrum';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 500);

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
 * Mark a transaction for rejection.
 *
 * @param txId - The txId as hash string.
 */
async function markTxForRejection(txHash: string, chain: SupportedChain) {
  const txId = hexStringToBytesArray(txHash);
  await using chainflip = await getChainflipApi();
  switch (chain) {
    case 'Bitcoin':
      return brokerMutex.runExclusive(async () =>
        chainflip.tx.bitcoinIngressEgress
          .markTransactionForRejection(txId.reverse())
          .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
      );
    case 'Ethereum':
      case 'Arbitrum':
      return brokerMutex.runExclusive(async () =>
        chainflip.tx.ethereumIngressEgress
          .markTransactionForRejection(txId)
          .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
      );
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
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
    case 'Ethereum': break;
    default: throw new Error(`Unssuported evm chain ${chain}`);
  };

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
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  reportFunction: (txId: string) => Promise<void>,
): Promise<string> {
  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress,
    minPriceX128: '0',
  };
  const swapParams = await requestNewSwap(
    'Btc',
    'Usdc',
    destinationAddressForUsdc,
    'brokerLevelScreeningTest',
    undefined,
    0,
    true,
    doBoost ? 100 : 0,
    refundParameters,
  );
  const txId = await sendBtc(swapParams.depositAddress, amount, 0);
  await reportFunction(txId);
  return swapParams.channelId.toString();
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
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  reportFunction: (txId: string) => Promise<void>,
): Promise<void> {
  const destinationAddressForUsdc = await newAssetAddress('Usdc');
  const txId = await buildAndSendBtcVaultSwap(parseFloat(amount), 'Usdc', destinationAddressForUsdc, refundAddress, {
    account: broker.address,
    commissionBps: 0,
  }, [], 0);
  await reportFunction(txId);
}

async function testBrokerLevelScreeningEthereum(
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const chain = chainFromAsset(sourceAsset);
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  testBrokerLevelScreening.log(`Testing broker level screening for ${chain} ${sourceAsset}...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const ethereumRefundAddress = await newAssetAddress('Eth');

  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: ethereumRefundAddress,
    minPriceX128: '0',
  };

  const swapParams = await requestNewSwap(
    sourceAsset,
    'Btc',
    destinationAddressForBtc,
    'brokerLevelScreeningTestEth',
    undefined,
    0,
    true,
    0,
    refundParameters,
  );

  if (sourceAsset === chainGasAsset('Ethereum') || sourceAsset === chainGasAsset('Arbitrum')) {
    await send(sourceAsset, swapParams.depositAddress);
    testBrokerLevelScreening.log(`Sent initial ${sourceAsset} tx...`);
    await observeEvent(`${ingressEgressPallet}:DepositFinalised`).event;
    testBrokerLevelScreening.log(`Initial deposit ${sourceAsset} received...`);
    // The first tx will cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // succesfully to make sure the Deposit contract is deployed.
    await waitForDepositContractDeployment(chain, swapParams.depositAddress);
  }

  testBrokerLevelScreening.log(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(sourceAsset, swapParams.depositAddress)).transactionHash as string;
  testBrokerLevelScreening.log(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  testBrokerLevelScreening.log(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(`${ingressEgressPallet}:TransactionRejectedByBroker`).event;

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
      `Didn't receive funds refund to address ${ethereumRefundAddress} within timeout!`,
    );
  }

  testBrokerLevelScreening.log(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

async function testBrokerLevelScreeningEthereumVaultSwap(
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const chain = chainFromAsset(sourceAsset);
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  testBrokerLevelScreening.log(`Testing broker level screening for ${chain} ${sourceAsset} vault swap...`);
  const MAX_RETRIES = 120;

  const destinationAddressForBtc = await newAssetAddress('Btc');
  const ethereumRefundAddress = await newAssetAddress('Eth');

  testBrokerLevelScreening.log(`Refund address for ${sourceAsset} is ${ethereumRefundAddress}...`);

  // const refundParameters: FillOrKillParamsX128 = {
  //   retryDurationBlocks: 0,
  //   refundAddress: ethereumRefundAddress,
  //   minPriceX128: '0',
  // };

  testBrokerLevelScreening.log(`Sending ${sourceAsset} (vault swap) tx to reject...`);
  const txHash = await executeEvmVaultSwap(
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
    ethereumRefundAddress
  );
  testBrokerLevelScreening.log(`Sent ${sourceAsset} (vault swap) tx...`);

  await reportFunction(txHash);
  testBrokerLevelScreening.log(`Marked ${sourceAsset} (vault swap) ${txHash} for rejection. Awaiting refund.`);

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

  testBrokerLevelScreening.log(`Marked ${sourceAsset} vault swap was rejected and refunded 👍.`);
}

async function testBrokerLevelScreeningEthereumLiquidityDeposit(
  sourceAsset: InternalAsset,
  reportFunction: (txId: string) => Promise<void>,
) {
  const chain = chainFromAsset(sourceAsset);
  const ingressEgressPallet = ingressEgressPalletForChain(chain);

  testBrokerLevelScreening.log(`Testing broker level screening for ${chain} ${sourceAsset}...`);
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

  testBrokerLevelScreening.log(`refund address is: ${ethereumRefundAddress}`);

  // Create new LP deposit address for //LP_1
  const eventHandle = observeEvent('liquidityProvider:LiquidityDepositAddressReady', {
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
  testBrokerLevelScreening.log(`Got deposit address: ${depositAddress}`);

  if (sourceAsset === chainGasAsset('Ethereum') || sourceAsset === chainGasAsset('Arbitrum')) {
    // The first tx cannot be rejected because we can't determine the txId for deposits to undeployed Deposit
    // contracts. We will reject the second transaction instead. We must wait until the fetch has been broadcasted
    // succesfully to make sure the Deposit contract is deployed.

    const amount = '3';
    const observeAccountCreditedEvent = observeEvent('assetBalances:AccountCredited', {
      test: (event) =>
        event.data.asset === sourceAsset &&
        isWithinOnePercent(
          BigInt(event.data.amountCredited.replace(/,/g, '')),
          BigInt(amountToFineAmountBigInt(amount, sourceAsset)),
        ),
    }).event;

    await send(sourceAsset, depositAddress, amount);
    testBrokerLevelScreening.log(`Sent initial ${sourceAsset} tx...`);
    await observeEvent(`${ingressEgressPallet}:DepositFinalised`).event;
    testBrokerLevelScreening.log(`Initial deposit ${sourceAsset} received...`);
    await observeAccountCreditedEvent;
    testBrokerLevelScreening.log(`Account credited for ${sourceAsset}...`);
    await waitForDepositContractDeployment(chain, depositAddress);
  }

  testBrokerLevelScreening.log(`Sending ${sourceAsset} tx to reject...`);
  const txHash = (await send(sourceAsset, depositAddress)).transactionHash as string;
  testBrokerLevelScreening.log(`Sent ${sourceAsset} tx...`);

  await reportFunction(txHash);
  testBrokerLevelScreening.log(`Marked ${sourceAsset} ${txHash} for rejection. Awaiting refund.`);

  await observeEvent(`${ingressEgressPallet}:TransactionRejectedByBroker`).event;

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
      `Didn't receive funds refund to address ${ethereumRefundAddress} within timeout!`,
    );
  }

  testBrokerLevelScreening.log(`Marked ${sourceAsset} transaction was rejected and refunded 👍.`);
}

// Sets the ingress_egress broker whitelist to the given `broker`.
async function setWhitelistedBroker(brokerAddress: Uint8Array) {
  const BTC_WHITELIST_PREFIX = '3ed3ce16dbc61ca64eaac5a96e809a8f6b8fb02fc586c9dab2385ea1690a7db6';
  const ETH_WHITELIST_PREFIX = '4fc967eb3d0785df0389312c2ebd853e6b8fb02fc586c9dab2385ea1690a7db6';
  const ARB_WHITELIST_PREFIX = '3d3491b8c14ff78a5176bc3b6ebe516f6b8fb02fc586c9dab2385ea1690a7db6'

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
async function testBrokerLevelScreeningBitcoin(testBoostedDeposits: boolean = false) {
  const MILLI_SECS_PER_BLOCK = 6000;

  // 1. -- Test no boost and early tx report --
  testBrokerLevelScreening.log('Testing broker level screening for Bitcoin with no boost...');
  let btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestBtc('0.2', false, btcRefundAddress, async (txId) =>
    setTxRiskScore(txId, 9.0),
  );

  await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  testBrokerLevelScreening.log(`Marked Bitcoin transaction was rejected and refunded 👍.`);

  if (testBoostedDeposits) {
    // 2. -- Test boost and early tx report --
    testBrokerLevelScreening.log(
      'Testing broker level screening for Bitcoin with boost and a early tx report...',
    );
    btcRefundAddress = await newAssetAddress('Btc');

    await brokerLevelScreeningTestBtc('0.2', true, btcRefundAddress, async (txId) =>
      setTxRiskScore(txId, 9.0),
    );
    await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;

    if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
      throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
    }
    testBrokerLevelScreening.log(`Marked Bitcoin transaction was rejected and refunded 👍.`);

    // 3. -- Test boost and late tx report --
    // Note: We expect the swap to be executed and not refunded because the tx was reported too late.
    testBrokerLevelScreening.log(
      'Testing broker level screening with boost and a late tx report...',
    );
    btcRefundAddress = await newAssetAddress('Btc');

    const channelId = await brokerLevelScreeningTestBtc(
      '0.2',
      true,
      btcRefundAddress,
      // We wait 12 seconds (2 localnet btc blocks) before we submit the tx.
      // We submit the extrinsic manually in order to ensure that even though it definitely arrives,
      // the transaction is refunded because the extrinsic is submitted too late.
      async (txId) => {
        await sleep(MILLI_SECS_PER_BLOCK * 2);
        await markTxForRejection(txId, 'Bitcoin');
      },
    );

    await observeEvent('bitcoinIngressEgress:DepositFinalised', {
      test: (event) => event.data.channelId === channelId,
    }).event;

    testBrokerLevelScreening.log(`Bitcoin swap was executed and transaction was not refunded 👍.`);
  }
}

async function testBrokerLevelScreeningBitcoinVaultSwap(testBoostedDeposits: boolean = false) {
  // -- Test vault swap rejection --
  testBrokerLevelScreening.log('Testing broker level screening for Bitcoin vault swap...');
  const btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestBtcVaultSwap('0.2', false, btcRefundAddress, async (txId) =>
    setTxRiskScore(txId, 9.0),
  );

  // Currently this event cannot be decoded correctly, so we don't wait for it,
  // just wait for the funds to arrive at the refund address
  // await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  testBrokerLevelScreening.log(`Bitcoin vault swap was rejected and refunded 👍.`);
}

async function main() {
  await ensureHealth();
  const previousMockmode = (await setMockmode('Manual')).previous;

  // NOTE: we don't test FLIP rejections because they are currently disabled in the
  // deposit monitor, since Elliptic doesn't provide Flip analysis.

  // test rejection of swaps by the responsible broker
  await Promise.all([
    testBrokerLevelScreeningBitcoin(),
    testBrokerLevelScreeningEthereum('Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereum('Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereum('Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereum('ArbUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereum('ArbEth', async (txId) => setTxRiskScore(txId, 9.0)),
  ]);

  // test rejection of LP deposits, this requires the rejecting broker to be whitelisted:
  await setWhitelistedBroker(broker.addressRaw);
  await Promise.all([
    testBrokerLevelScreeningEthereumLiquidityDeposit('Eth', async (txId) =>
      setTxRiskScore(txId, 9.0),
    ),
    testBrokerLevelScreeningEthereumLiquidityDeposit('Usdt', async (txId) =>
      setTxRiskScore(txId, 9.0),
    ),
    testBrokerLevelScreeningEthereumLiquidityDeposit('Usdc', async (txId) =>
      setTxRiskScore(txId, 9.0),
    ),
    testBrokerLevelScreeningEthereumLiquidityDeposit('ArbEth', async (txId) =>
      setTxRiskScore(txId, 9.0),
    ),
    testBrokerLevelScreeningEthereumLiquidityDeposit('ArbUsdc', async (txId) =>
      setTxRiskScore(txId, 9.0),
    ),
  ]);

  // test vault swaps
  await openPrivateBtcChannel('//BROKER_1');
  await Promise.all([
    testBrokerLevelScreeningBitcoinVaultSwap(),
    testBrokerLevelScreeningEthereumVaultSwap('Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereumVaultSwap('Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereumVaultSwap('ArbEth', async (txId) => setTxRiskScore(txId, 9.0)),
    testBrokerLevelScreeningEthereumVaultSwap('ArbUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
  ]);

  await setMockmode(previousMockmode);
}
