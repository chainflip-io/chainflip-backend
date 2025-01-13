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
import { getChainflipApi } from '../shared/utils/substrate';
import { fundFlip } from '../shared/fund_flip';

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
  commissionBps: number = 0,
) {
  await using chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);

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
  await waitForBtcTransaction(txid);

  return txid;
}

export async function openPrivateBtcChannel(brokerUri: string) {
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
    await brokerMutex.runExclusive(async () => {
      await chainflip.tx.swapping
        .openPrivateBtcChannel()
        .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
    });
    console.log('Private Btc channel opened');
  }
}

export async function registerAffiliate(
  brokerUri: string,
  affiliateUri: string,
  affiliateShortId: number,
) {
  const chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);
  const affiliate = createStateChainKeypair(affiliateUri);

  await brokerMutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .registerAffiliate(affiliate.address, affiliateShortId)
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
  });
}
