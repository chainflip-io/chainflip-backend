import assert from 'assert';
import { Chains } from '@chainflip/cli';
import { waitForBtcTransaction, sendVaultTransaction } from '../shared/send_btc';
import {
  Asset,
  assetDecimals,
  brokerMutex,
  chainFromAsset,
  createStateChainKeypair,
  decodeDotAddressForContract,
  fineAmountToAmount,
  handleSubstrateError,
  stateChainAssetFromAsset,
} from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { fundFlip } from '../shared/fund_flip';

interface BtcVaultSwapDetails {
  chain: string;
  nulldata_payload: string;
  deposit_address: string;
}

interface BtcVaultSwapExtraParameters {
  chain: 'Bitcoin';
  min_output_amount: string;
  retry_duration: number;
}

export async function buildAndSendBtcVaultSwap(
  depositAmountBtc: number,
  destinationAsset: Asset,
  destinationAddress: string,
  refundAddress: string,
  brokerFees: {
    account: string;
    commissionBps: number;
  },
  affiliateFees: {
    account: string;
    bps: number;
  }[] = [],
) {
  await using chainflip = await getChainflipApi();

  const extraParameters: BtcVaultSwapExtraParameters = {
    chain: 'Bitcoin',
    min_output_amount: '0',
    retry_duration: 0,
  };

  const BtcVaultSwapDetails = (await chainflip.rpc(
    `cf_get_vault_swap_details`,
    brokerFees.account,
    { chain: 'Bitcoin', asset: stateChainAssetFromAsset('Btc') },
    { chain: chainFromAsset(destinationAsset), asset: stateChainAssetFromAsset(destinationAsset) },
    chainFromAsset(destinationAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress,
    brokerFees.commissionBps,
    extraParameters,
    null, // channel_metadata
    0, // boost_fee
    affiliateFees,
    null, // dca_params
  )) as unknown as BtcVaultSwapDetails;

  assert.strictEqual(BtcVaultSwapDetails.chain, 'Bitcoin');

  const txid = await sendVaultTransaction(
    BtcVaultSwapDetails.nulldata_payload,
    depositAmountBtc,
    BtcVaultSwapDetails.deposit_address,
    refundAddress,
  );
  await waitForBtcTransaction(txid);

  return txid;
}

export async function openPrivateBtcChannel(brokerUri: string): Promise<number> {
  // Check if the channel is already open
  const chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);
  const existingPrivateChannel = Number(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.swapping.brokerPrivateBtcChannels(broker.address)) as any,
  );
  if (existingPrivateChannel) {
    return existingPrivateChannel;
  }

  // Fund the broker the required bond amount for opening a private channel
  const fundAmount = fineAmountToAmount(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.swapping.brokerBond()) as any as string,
    assetDecimals('Flip'),
  );
  await fundFlip(broker.address, fundAmount);

  // Open the private channel
  const openedChannelEvent = observeEvent('swapping:PrivateBrokerChannelOpened', {
    test: (event) => event.data.brokerId === broker.address,
  }).event;
  await brokerMutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .openPrivateBtcChannel()
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  return Number((await openedChannelEvent).data.channelId);
}

export async function registerAffiliate(
  brokerUri: string,
  affiliateShortId: number,
  withdrawalAddress: string,
) {
  const chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);

  const registeredEvent = observeEvent('swapping:AffiliateRegistration', {
    test: (event) =>
      event.data.brokerId === broker.address &&
      Number(event.data.affiliateShortId) === affiliateShortId,
  }).event;

  await brokerMutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .registerAffiliate(affiliateShortId, withdrawalAddress)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  return registeredEvent;
}
