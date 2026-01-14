import assert from 'assert';
import { sendVaultTransaction } from 'shared/send_btc';
import {
  Asset,
  assetDecimals,
  Assets,
  cfMutex,
  chainFromAsset,
  Chains,
  createStateChainKeypair,
  decodeDotAddressForContract,
  fineAmountToAmount,
  stateChainAssetFromAsset,
  waitForExt,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
import { Logger } from 'shared/utils/logger';

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

async function openPrivateBtcChannel(logger: Logger, brokerUri: string): Promise<number> {
  const release = await cfMutex.acquire(brokerUri);
  // Check if the channel is already open
  const chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);
  const existingPrivateChannel = Number(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.swapping.brokerPrivateBtcChannels(broker.address)) as any,
  );
  if (existingPrivateChannel) {
    release();
    return existingPrivateChannel;
  }

  // Fund the broker the required bond amount for opening a private channel
  const fundAmount = fineAmountToAmount(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await chainflip.query.swapping.brokerBond()) as any as string,
    assetDecimals('Flip'),
  );
  await fundFlip(logger, broker.address, fundAmount);

  // Open the private channel
  logger.debug('Opening private BTC channel');
  const { promise, waiter } = waitForExt(chainflip, logger, 'InBlock');
  const nonce = (await chainflip.rpc.system.accountNextIndex(broker.address)) as unknown as number;
  const unsub = await chainflip.tx.swapping
    .openPrivateBtcChannel()
    .signAndSend(broker, { nonce }, waiter);
  const events = await promise;
  unsub();
  release();

  const { channelId } = events
    .find(
      ({ event }) => event.section === 'swapping' && event.method === 'PrivateBrokerChannelOpened',
    )!
    .event.toHuman() as unknown as { channelId: string };
  return Number(channelId);
}

export async function buildAndSendBtcVaultSwap(
  logger: Logger,
  brokerUri: string,
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

  await openPrivateBtcChannel(logger, brokerUri);
  const broker = createStateChainKeypair(brokerUri);

  const extraParameters: BtcVaultSwapExtraParameters = {
    chain: 'Bitcoin',
    min_output_amount: '0',
    retry_duration: 0,
  };

  logger.trace('Requesting vault swap parameter encoding');
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

  logger.trace('Sending BTC vault swap transaction');
  const txid = await sendVaultTransaction(
    logger,
    BtcVaultSwapDetails.nulldata_payload,
    depositAmountBtc,
    BtcVaultSwapDetails.deposit_address,
    refundAddress,
  );

  return txid;
}
export async function buildAndSendInvalidBtcVaultSwap(
  logger: Logger,
  brokerUri: string,
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

  await openPrivateBtcChannel(logger, brokerUri);
  const broker = createStateChainKeypair(brokerUri);

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
    logger,
    // wrong encoded payload
    '0x6a3a0101701b90c687681727ada344f4e440f1a82ae548f66400602b04924ddf21970000000000000000ff010002001e02003d7c6c69666949c04689',
    depositAmountBtc,
    BtcVaultSwapDetails.deposit_address,
    refundAddress,
  );

  return txid;
}

export async function registerAffiliate(
  logger: Logger,
  brokerUri: string,
  withdrawalAddress: string,
) {
  const chainflip = await getChainflipApi();
  const broker = createStateChainKeypair(brokerUri);

  logger.trace('Registering affiliate');
  const release = await cfMutex.acquire(brokerUri);
  const { promise, waiter } = waitForExt(chainflip, logger, 'InBlock', release);
  const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
  const unsub = await chainflip.tx.swapping
    .registerAffiliate(withdrawalAddress)
    .signAndSend(broker, { nonce }, waiter);

  const events = await promise;
  unsub();

  return events
    .find(({ event }) => event.section === 'swapping' && event.method === 'AffiliateRegistration')!
    .event.data.toHuman() as {
    shortId: number;
    affiliateId: string;
  };
}
