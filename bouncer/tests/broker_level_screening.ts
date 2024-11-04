import { ExecutableTest } from '../shared/executable_test';
import { sendBtcAndReturnTxId } from '../shared/send_btc';
import { randomBytes } from 'crypto';
import { hexStringToBytesArray, newAddress, sleep } from '../shared/utils';
import { handleSubstrateError } from '../shared/utils';
import { brokerMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import Keyring from '../polkadot/keyring';
import { requestNewSwap } from '../shared/perform_swap';
import { execSync } from 'child_process';
import { FillOrKillParamsX128 } from '../shared/new_swap';
import { getBtcBalance } from '../shared/get_btc_balance';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

export const testBrokerLevelScreening = new ExecutableTest('Broker-Level-Screening', main, 300);

/**
 * Observes the balance of a BTC address and returns true if the balance changes.
 *
 * @param address - The address to observe the balance of.
 * @returns - Whether the balance changed.
 */
async function observeBtcAddressBalanceChange(address: string): Promise<boolean> {
  const MAX_RETRIES = 100;
  let retryCount = 0;
  while (true) {
    await sleep(1000);
    let balance = await getBtcBalance(address);
    if (balance !== 0) {
      return Promise.resolve(true);
    }
    retryCount++;
    if (retryCount > MAX_RETRIES) {
      console.error(`BTC balance for ${address} did not change after ${MAX_RETRIES} seconds.`);
      return Promise.resolve(false);
    }
  }
}

/**
 * Submits a transaction as tainted to the extrinsic on the state chain.
 *
 * @param tx_id - The tx_id to submit as tainted as byte array in the correct order.
 */
async function submitTxAsTainted(tx_id: number[]) {
  await using chainflip = await getChainflipApi();
  return brokerMutex.runExclusive(async () =>
    chainflip.tx.bitcoinIngressEgress
      .markTransactionAsTainted(tx_id)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
}

/**
 * Pauses or resumes the bitcoin block production. We send a command to the docker container to start or stop mining blocks.
 *
 *
 * @param command - Whether to pause or resume the block production.
 * @returns - Whether the command was successful.
 */
function pauseBtcBlockProduction(command: boolean): boolean {
  let start = 'docker exec bitcoin rm /root/mine_blocks';
  let stop = 'docker exec bitcoin touch /root/mine_blocks';
  try {
    execSync(command ? start : stop);
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
 * @param stopBlockProductionFor - The number of blocks to stop block production for. We need this to ensure that the tainted tx is on chain before the deposit is witnessed/prewitnessed.
 * @param waitBeforeReport - The number of milliseconds to wait before reporting the tx as tainted.
 */
async function brokerLevelScreeningTestScenario(
  amount: string,
  doBoost: boolean,
  refundAddress: string,
  stopBlockProductionFor = 0,
  waitBeforeReport = 0,
) {
  let destinationAddressForUsdc = await newAddress('Usdc', randomBytes(32).toString('hex'));
  const refundParameters: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: refundAddress,
    minPriceX128: '0',
  };
  const swapParams = await requestNewSwap(
    'Btc',
    'Usdc',
    destinationAddressForUsdc,
    'test',
    undefined,
    0,
    true,
    doBoost ? 100 : 0,
    refundParameters,
  );
  let tx_id = await sendBtcAndReturnTxId(swapParams.depositAddress, amount);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(true);
  }
  await sleep(waitBeforeReport);
  // Note: The bitcoin core js lib returns the tx_id in reverse order.
  // On chain we expect the tx_id to be in the correct order (like the Bitcoin internal representation).
  // Because of this we need to reverse the tx_id before submitting it as tainted.
  await submitTxAsTainted(hexStringToBytesArray(tx_id).reverse());
  await sleep(stopBlockProductionFor);
  if (stopBlockProductionFor > 0) {
    pauseBtcBlockProduction(false);
  }
}

async function main() {
  const MILLI_SECS_PER_BLOCK = 6000;
  const BLOCKS_TO_WAIT = 2;

  //1. -- Test no boost and early tx report --
  testBrokerLevelScreening.log('Testing broker level screening with no boost...');
  let btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));

  await brokerLevelScreeningTestScenario(
    '0.2',
    false,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );

  await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;
  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(
      `Didn't receive funds refund to address ${btcRefundAddress} within the timeout!`,
    );
  }

  testBrokerLevelScreening.log(`Tainted transaction was rejected and refunded 👍.`);

  //2. -- Test boost and early tx report --
  testBrokerLevelScreening.log(
    'Testing broker level screening with boost and a early tx report...',
  );
  btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));

  await brokerLevelScreeningTestScenario(
    '0.2',
    true,
    btcRefundAddress,
    MILLI_SECS_PER_BLOCK * BLOCKS_TO_WAIT,
  );
  await observeEvent('bitcoinIngressEgress:TaintedTransactionRejected').event;

  if (!(await observeBtcAddressBalanceChange(btcRefundAddress))) {
    throw new Error(
      `Didn't receive funds refund to address ${btcRefundAddress} within the timeout!`,
    );
  }
  testBrokerLevelScreening.log(`Tainted transaction was rejected and refunded 👍.`);

  //3. -- Test boost and late tx report --
  // Note: We expect the swap to be executed and not refunded because the tainted tx is on chain.
  testBrokerLevelScreening.log('Testing broker level screening with boost and a late tx report...');
  btcRefundAddress = await newAddress('Btc', randomBytes(32).toString('hex'));

  await brokerLevelScreeningTestScenario('0.2', true, btcRefundAddress, MILLI_SECS_PER_BLOCK * 2);
  await observeEvent('swapping:SwapExecuted').event;

  testBrokerLevelScreening.log(`Swap was executed and tainted transaction was not refunded 👍.`);
}
