import { randomBytes } from 'crypto';
import { execSync } from 'child_process';
import { InternalAsset } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { sendBtcAndReturnTxId } from '../shared/send_btc';
import {
  hexStringToBytesArray,
  newAddress,
  sleep,
  handleSubstrateError,
  brokerMutex,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 300);

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
 * @param txId - The txId as a byte array in 'unreversed' order - which
 * is reverse of how it's normally displayed in bitcoin block explorers.
 */
async function markTxForRejection(txId: number[]) {
  await using chainflip = await getChainflipApi();
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.bitcoinIngressEgress
      .markTransactionForRejection(txId)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}

/**
 * Pauses or resumes the bitcoin block production. We send a command to the docker container to start or stop mining blocks.
 *
 * @param pause - Whether to pause or resume the block production.
 * @returns - Whether the command was successful.
 */
function pauseBtcBlockProduction(pause: boolean): boolean {
  try {
    execSync(
      pause
        ? 'docker exec bitcoin rm /root/mine_blocks'
        : 'docker exec bitcoin touch /root/mine_blocks',
    );
    return true;
  } catch (error) {
    console.error(error);
    return false;
  }
}

/**
 * Runs a test scenario for broker level screening based on the given parameters.
 *
 * @param amount - The deposit amount.
 * @param doBoost - Whether to boost the deposit.
 * @param refundAddress - The address to refund to.
 * @param stopBlockProductionFor - The number of blocks to stop block production for. We need this to ensure that the marked tx is on chain before the deposit is witnessed/prewitnessed.
 * @param waitBeforeReport - The number of milliseconds to wait before reporting the tx.
 * @returns - The the channel id of the deposit channel.
 */
async function brokerLevelScreeningTestScenario(
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  stopBlockProductionFor = 0,
  waitBeforeReport = 0,
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
  const txId = await sendBtcAndReturnTxId(swapParams.depositAddress, amount);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(true);
  }
  await sleep(waitBeforeReport);
  // Note: The bitcoin core js lib returns the txId in reverse order.
  // On chain we expect the txId to be in the correct order (like the Bitcoin internal representation).
  // Because of this we need to reverse the txId before marking it for rejection.
  await markTxForRejection(hexStringToBytesArray(txId).reverse());
  await sleep(stopBlockProductionFor);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(false);
  }
  return Promise.resolve(swapParams.channelId.toString());
}

// -- Test suite for broker level screening --
//
// In this tests we are interested in the following scenarios:
//
// 1. No boost and early tx report -> tx is reported early and the swap is refunded.
// 2. Boost and early tx report -> tx is reported early and the swap is refunded.
// 3. Boost and late tx report -> tx is reported late and the swap is not refunded.
async function main() {
  const MILLI_SECS_PER_BLOCK = 6000;
  const BLOCKS_TO_WAIT = 2;

  // 1. -- Test no boost and early tx report --
  testBrokerLevelScreening.log('Testing broker level screening with no boost...');
  let btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario(
    '0.2',
    false,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );

  await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }

  testBrokerLevelScreening.log(`Marked transaction was rejected and refunded 👍.`);

  // 2. -- Test boost and early tx report --
  testBrokerLevelScreening.log(
    'Testing broker level screening with boost and a early tx report...',
  );
  btcRefundAddress = await newAssetAddress('Btc');

  await brokerLevelScreeningTestScenario(
    '0.2',
    true,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );
  await observeEvent('bitcoinIngressEgress:TransactionRejectedByBroker').event;

  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(`Didn't receive funds refund to address ${btcRefundAddress} within timeout!`);
  }
  testBrokerLevelScreening.log(`Marked transaction was rejected and refunded 👍.`);

  // 3. -- Test boost and late tx report --
  // Note: We expect the swap to be executed and not refunded because the tx was reported too late.
  testBrokerLevelScreening.log('Testing broker level screening with boost and a late tx report...');
  btcRefundAddress = await newAssetAddress('Btc');

  const channelId = await brokerLevelScreeningTestScenario(
    '0.2',
    true,
    btcRefundAddress,
    0,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );

  await observeEvent('bitcoinIngressEgress:DepositFinalised', {
    test: (event) => event.data.channelId === channelId,
  }).event;

  testBrokerLevelScreening.log(`Swap was executed and transaction was not refunded 👍.`);
}
