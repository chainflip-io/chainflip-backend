import assert from 'assert';
import { Chains } from '@chainflip/cli';
import { ExecutableTest } from '../shared/executable_test';
import { BTC_ENDPOINT, waitForBtcTransaction, sendVaultTransaction } from '../shared/send_btc';
import {
  amountToFineAmount,
  Asset,
  assetDecimals,
  chainFromAsset,
  createStateChainKeypair,
  decodeDotAddressForContract,
  fineAmountToAmount,
  newAddress,
  observeBalanceIncrease,
  stateChainAssetFromAsset,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { getBalance } from '../shared/get_balance';
import { brokerApiRpc } from '../shared/json_rpc';
import { getEarnedBrokerFees } from './broker_fee_collection';
import { fundFlip } from '../shared/fund_flip';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testBtcVaultSwap = new ExecutableTest('Btc-Vault-Swap', main, 120);

// Fee to use for the broker and affiliates
const commissionBps = 100;

interface BtcVaultSwapDetails {
  chain: string;
  nulldata_payload: string;
  deposit_address: string;
  expires_at: number;
}

interface BtcVaultSwapExtraParameters {
  chain: 'Bitcoin';
  min_output_amount: string;
  retry_duration: number;
}

interface Beneficiary {
  account: string;
  bps: number;
}

export async function buildAndSendBtcVaultSwap(
  depositAmountBtc: number,
  brokerUri: string,
  destinationAsset: Asset,
  destinationAddress: string,
  refundAddress: string,
  affiliateAddresses: string[],
) {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair(brokerUri);
  testBtcVaultSwap.debugLog('Broker:', broker.address);
  testBtcVaultSwap.debugLog(`Btc endpoint is set to`, BTC_ENDPOINT);

  const affiliates: Beneficiary[] = [];
  for (const affiliateAddress of affiliateAddresses) {
    affiliates.push({ account: affiliateAddress, bps: commissionBps });
  }

  const extraParameters: BtcVaultSwapExtraParameters = {
    chain: 'Bitcoin',
    min_output_amount: '0',
    retry_duration: 0,
  };

  const BtcVaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    broker.address,
    { chain: 'Bitcoin', asset: stateChainAssetFromAsset('Btc') },
    { chain: chainFromAsset(destinationAsset), asset: stateChainAssetFromAsset(destinationAsset) },
    chainFromAsset(destinationAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress,
    commissionBps, // broker_commission
    extraParameters,
    null, // channel_metadata
    0, // boost_fee
    affiliates,
    null, // dca_params
  )) as unknown as BtcVaultSwapDetails;

  assert.strictEqual(BtcVaultSwapDetails.chain, 'Bitcoin');
  testBtcVaultSwap.debugLog('nulldata_payload:', BtcVaultSwapDetails.nulldata_payload);
  testBtcVaultSwap.debugLog('deposit_address:', BtcVaultSwapDetails.deposit_address);
  testBtcVaultSwap.debugLog('expires_at:', BtcVaultSwapDetails.expires_at);

  // Calculate expected expiry time assuming block time is 6 secs, expires_at = time left to next rotation
  const epochDuration = (await chainflip.rpc(`cf_epoch_duration`)) as number;
  const epochStartedAt = (await chainflip.rpc(`cf_current_epoch_started_at`)) as number;
  const currentBlockNumber = (await chainflip.rpc.chain.getHeader()).number.toNumber();
  const blocksUntilNextRotation = epochDuration + epochStartedAt - currentBlockNumber;
  const expectedExpiresAt = Date.now() + blocksUntilNextRotation * 6000;
  // Check that expires_at field is correct (within 20 secs drift)
  assert(
    Math.abs(expectedExpiresAt - BtcVaultSwapDetails.expires_at) <= 20 * 1000,
    `BtcVaultSwapDetails expiry timestamp is not within a 20 secs drift of the expected expiry time.
      expectedExpiresAt = ${expectedExpiresAt} and actualExpiresAt = ${BtcVaultSwapDetails.expires_at}`,
  );

  const txid = await sendVaultTransaction(
    BtcVaultSwapDetails.nulldata_payload,
    depositAmountBtc,
    BtcVaultSwapDetails.deposit_address,
    refundAddress,
  );
  testBtcVaultSwap.log('Broadcast successful, txid:', txid);

  await waitForBtcTransaction(txid);
  testBtcVaultSwap.debugLog('Transaction confirmed');
  return txid;
}

async function testVaultSwap(
  depositAmountBtc: number,
  brokerUri: string,
  destinationAsset: Asset,
  affiliateUri: string,
) {
  // Addresses
  const destinationAddress = await newAddress(destinationAsset, 'BTC_VAULT_SWAP');
  testBtcVaultSwap.debugLog('destinationAddress:', destinationAddress);
  const refundAddress = await newAddress('Btc', 'BTC_VAULT_SWAP_REFUND');
  testBtcVaultSwap.debugLog('Refund address:', refundAddress);

  // Amounts before swap
  const destinationAmountBeforeSwap = await getBalance(destinationAsset, destinationAddress);
  const broker = createStateChainKeypair(brokerUri);
  const affiliate = createStateChainKeypair(affiliateUri);
  const earnedBrokerFeesBefore = await getEarnedBrokerFees(broker);
  const earnedAffiliateFeesBefore = await getEarnedBrokerFees(affiliate);
  testBtcVaultSwap.debugLog('Earned broker fees before:', earnedBrokerFeesBefore);
  testBtcVaultSwap.debugLog('Earned affiliate fees before:', earnedAffiliateFeesBefore);

  const observeSwapExecutedEvent = observeEvent(`swapping:SwapExecuted`, {
    test: (event) =>
      event.data.inputAsset === 'Btc' &&
      event.data.outputAsset === destinationAsset &&
      event.data.inputAmount.replace(/,/g, '') ===
        amountToFineAmount(depositAmountBtc.toString(), assetDecimals('Btc')),
  }).event;

  await buildAndSendBtcVaultSwap(
    depositAmountBtc,
    brokerUri,
    destinationAsset,
    destinationAddress,
    refundAddress,
    [affiliate.address],
  );

  // Complete swap and check balance
  testBtcVaultSwap.debugLog('Waiting for swap executed event');
  await observeSwapExecutedEvent;
  testBtcVaultSwap.log(`Btc -> ${destinationAsset} Vault Swap executed`);
  await observeBalanceIncrease(destinationAsset, destinationAddress, destinationAmountBeforeSwap);
  testBtcVaultSwap.log(`Balance increased, Vault Swap Complete`);

  // Check that both the broker and affiliate earned fees
  const earnedBrokerFeesAfter = await getEarnedBrokerFees(broker);
  const earnedAffiliateFeesAfter = await getEarnedBrokerFees(affiliate);
  testBtcVaultSwap.debugLog('Earned broker fees after:', earnedBrokerFeesAfter);
  testBtcVaultSwap.debugLog('Earned affiliate fees after:', earnedAffiliateFeesAfter);
  assert(earnedBrokerFeesAfter > earnedBrokerFeesBefore, 'No increase in earned broker fees');
  assert(
    earnedAffiliateFeesAfter > earnedAffiliateFeesBefore,
    'No increase in earned affiliate fees',
  );
}

export async function openPrivateBtcChannel(brokerUri: string) {
  // TODO: Use chainflip SDK instead so we can support any broker uri
  assert.strictEqual(brokerUri, '//BROKER_1', 'Support for other brokers is not implemented');

  // Check if the channel is already open
  const chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);
  const existingPrivateChannel = Number(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.swapping.brokerPrivateBtcChannels(broker.address)) as any,
  );

  if (!existingPrivateChannel) {
    // Fund the broker the required bond amount for opening a private channel
    const fundAmount = fineAmountToAmount(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (await chainflip.query.swapping.brokerBond()) as any as string,
      assetDecimals('Flip'),
    );
    await fundFlip(broker.address, fundAmount);

    // Open the private channel
    try {
      await brokerApiRpc('broker_open_private_btc_channel', []);
      console.log('Private Btc channel opened');
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } catch (error: any) {
      // Ignore the error if the channel already exists
      if (error.message.includes('PrivateChannelExistsForBroker')) {
        console.log('Tried to open private Btc channel but one already exists', error);
      } else {
        // Any other error is unexpected, ie InsufficientFunds
        console.log(`Failed to open private Btc channel for ${brokerUri}`);
        throw error;
      }
    }
  }
}

async function registerAffiliate(brokerUri: string, affiliateUri: string) {
  // TODO: Use chainflip SDK instead so we can support any broker uri
  assert.strictEqual(brokerUri, '//BROKER_1', 'Support for other brokers is not implemented');

  const affiliate = createStateChainKeypair(affiliateUri);
  return brokerApiRpc('broker_register_affiliate', [affiliate.address]);
}

async function main() {
  const btcDepositAmount = 0.1;
  // TODO: Fee collection will work properly when using 'BROKER_1' and 'BROKER_2' because it will be effected by the other tests.
  const brokerUri = '//BROKER_1';
  const affiliateUri = '//BROKER_2';

  await openPrivateBtcChannel(brokerUri);
  await registerAffiliate(brokerUri, affiliateUri);
  await testVaultSwap(btcDepositAmount, brokerUri, 'Flip', affiliateUri);
}
