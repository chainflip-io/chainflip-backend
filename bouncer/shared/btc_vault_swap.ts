import assert from 'assert';
import { sendVaultTransaction } from 'shared/send_btc';
import {
  Asset,
  assetDecimals,
  Assets,
  chainFromAsset,
  Chains,
  decodeDotAddressForContract,
  fineAmountToAmount,
  stateChainAssetFromAsset,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
import { ChainflipIO, WithBrokerAccount } from 'shared/utils/chainflip_io';
import { swappingAffiliateRegistration } from 'generated/events/swapping/affiliateRegistration';
import { swappingPrivateBrokerChannelOpened } from 'generated/events/swapping/privateBrokerChannelOpened';
import { cfMutex } from 'shared/accounts';

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

async function getExistingPrivateBtcChannel(brokerAddress: string): Promise<number | undefined> {
  await using chainflip = await getChainflipApi();

  const existingPrivateChannel = Number(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.swapping.brokerPrivateBtcChannels(brokerAddress)) as any,
  );

  if (existingPrivateChannel) {
    return existingPrivateChannel;
  }
  return undefined;
}

export async function openPrivateBtcChannel<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  fundAccountWithBrokerBond = false,
): Promise<number> {
  const broker = cf.requirements.account;
  await using chainflip = await getChainflipApi();

  // Acquire mutex and check if broker already has private channel
  const release = await cfMutex.acquire(broker.uri);
  const existingPrivateChannel = await getExistingPrivateBtcChannel(broker.keypair.address);
  release();
  if (existingPrivateChannel) {
    return existingPrivateChannel;
  }

  if (fundAccountWithBrokerBond) {
    // Fund the broker the required bond amount for opening a private channel
    const fundAmount = fineAmountToAmount(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (await chainflip.query.swapping.brokerBond()) as any as string,
      assetDecimals('Flip'),
    );
    await fundFlip(cf, broker.keypair.address, fundAmount);
  }

  // Open the private channel
  cf.trace('Opening private BTC channel');

  try {
    const privateBrokerChannelOpenedEvent = await cf.submitExtrinsic({
      extrinsic: (api) => api.tx.swapping.openPrivateBtcChannel(),
      expectedEvent: {
        name: 'Swapping.PrivateBrokerChannelOpened',
        schema: swappingPrivateBrokerChannelOpened.refine(
          (event) => event.brokerId === broker.keypair.address,
        ),
      },
      filteredError: 'swapping.PrivateChannelExistsForBroker',
    });

    cf.debug(
      `Private BTC channel successfully opened for broker: ${privateBrokerChannelOpenedEvent.brokerId}`,
    );
    return Number(privateBrokerChannelOpenedEvent.channelId);
  } catch (err) {
    // Fetch the private channel instead, if the extrinsic fails
    if (err instanceof Error && err.message.includes('swapping.PrivateChannelExistsForBroker')) {
      const privateChannel = await getExistingPrivateBtcChannel(broker.keypair.address);
      cf.warn(`got an error fetching private channel: ${privateChannel}`);

      if (privateChannel) {
        return privateChannel;
      }
      throw Error(`Unexpected error private Btc channel should exists for broker: ${broker.uri}`);
    }
    throw err;
  }
}

export async function buildAndSendBtcVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  depositAmountBtc: number,
  destinationAsset: Asset,
  destinationAddress: string,
  refundAddress: string,
  brokerFee: number,
  affiliateFees: {
    account: string;
    bps: number;
  }[] = [],
) {
  await using chainflip = await getChainflipApi();

  await openPrivateBtcChannel(cf);
  const broker = cf.requirements.account.keypair;

  const extraParameters: BtcVaultSwapExtraParameters = {
    chain: 'Bitcoin',
    min_output_amount: '0',
    retry_duration: 0,
  };

  cf.trace('Requesting vault swap parameter encoding');
  const BtcVaultSwapDetails = (await chainflip.rpc(
    `cf_request_swap_parameter_encoding`,
    broker.address,
    stateChainAssetFromAsset(Assets.Btc),
    stateChainAssetFromAsset(destinationAsset),
    chainFromAsset(destinationAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress,
    brokerFee,
    extraParameters,
    null, // channel_metadata
    0, // boost_fee
    affiliateFees,
    null, // dca_params
  )) as unknown as BtcVaultSwapDetails;

  assert.strictEqual(BtcVaultSwapDetails.chain, 'Bitcoin');

  cf.trace('Sending BTC vault swap transaction');
  const txid = await sendVaultTransaction(
    BtcVaultSwapDetails.nulldata_payload,
    depositAmountBtc,
    BtcVaultSwapDetails.deposit_address,
    refundAddress,
  );

  return txid;
}
export async function buildAndSendInvalidBtcVaultSwap<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  depositAmountBtc: number,
  destinationAsset: Asset,
  destinationAddress: string,
  refundAddress: string,
  brokerFee: number,
  affiliateFees: {
    account: string;
    bps: number;
  }[] = [],
) {
  await using chainflip = await getChainflipApi();

  await openPrivateBtcChannel(cf);
  const broker = cf.requirements.account.keypair;

  const extraParameters: BtcVaultSwapExtraParameters = {
    chain: 'Bitcoin',
    min_output_amount: '0',
    retry_duration: 0,
  };

  const BtcVaultSwapDetails = (await chainflip.rpc(
    `cf_request_swap_parameter_encoding`,
    broker.address,
    stateChainAssetFromAsset(Assets.Btc),
    stateChainAssetFromAsset(destinationAsset),
    chainFromAsset(destinationAsset) === Chains.Polkadot
      ? decodeDotAddressForContract(destinationAddress)
      : destinationAddress,
    brokerFee,
    extraParameters,
    null, // channel_metadata
    0, // boost_fee
    affiliateFees,
    null, // dca_params
  )) as unknown as BtcVaultSwapDetails;

  assert.strictEqual(BtcVaultSwapDetails.chain, 'Bitcoin');

  const txid = await sendVaultTransaction(
    // wrong encoded payload
    '0x6a3a0101701b90c687681727ada344f4e440f1a82ae548f66400602b04924ddf21970000000000000000ff010002001e02003d7c6c69666949c04689',
    depositAmountBtc,
    BtcVaultSwapDetails.deposit_address,
    refundAddress,
  );

  return txid;
}

export async function registerAffiliate<A extends WithBrokerAccount>(
  cf: ChainflipIO<A>,
  withdrawalAddress: string,
) {
  const brokerUri = cf.requirements.account.uri;

  cf.trace('Registering affiliate');
  const affiliateRegistration = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.swapping.registerAffiliate(withdrawalAddress),
    expectedEvent: {
      name: 'Swapping.AffiliateRegistration',
      schema: swappingAffiliateRegistration.refine(
        (event) =>
          event.brokerId === cf.requirements.account.keypair.address &&
          event.withdrawalAddress === withdrawalAddress.toLowerCase(),
      ),
    },
  });

  cf.debug(
    `Affiliate with withdrawalAddress: ${withdrawalAddress} successfully registered for broker: ${brokerUri}`,
  );

  return affiliateRegistration;
}
