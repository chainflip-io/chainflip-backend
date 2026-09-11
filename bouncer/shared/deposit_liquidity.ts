import {
  newAssetAddress,
  decodeDotAddressForContract,
  amountToFineAmountBigInt,
  fineAmountToAmount,
  chainFromAsset,
  decodeSolAddress,
  assetDecimals,
  runWithTimeout,
  doAddressesMatch,
  chainGasAsset,
  encodedAddress,
  Chain,
  Asset,
} from 'shared/utils';
import { send } from 'shared/send';
import { getChainflipApi } from 'shared/utils/substrate';
import { liquidityProviderLiquidityDepositAddressReadyEvent } from 'generated/events/liquidityProvider/liquidityDepositAddressReady';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';
import { liquidityProviderLiquidityRefundAddressRegisteredEvent } from 'generated/events/liquidityProvider/liquidityRefundAddressRegistered';
import { ingressEgressDepositFinalisedEvent } from 'generated/events/generic/ingressEgress/depositFinalised';

export async function registerLiquidityRefundAddressForChain<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  chain: Chain,
  forceRegister = false,
) {
  const lpuri = cf.requirements.account.uri;
  const lp = cf.requirements.account.keypair;

  // Check if the refund address is already registered for this chain. If so, return early.
  if (!forceRegister) {
    await using chainflip = await getChainflipApi();
    const currentRefundAddress = await chainflip.query.assetBalances.refundAddresses([
      lp.address,
      chain,
    ]);
    if (currentRefundAddress !== undefined) {
      cf.debug(`Liquidity Refund Address already registered for ${lpuri} chain: ${chain}`);
      return;
    }
  }

  let refundAddress = await newAssetAddress(chainGasAsset(chain), lpuri);
  refundAddress = chain === 'Assethub' ? decodeDotAddressForContract(refundAddress) : refundAddress;
  refundAddress = chain === 'Solana' ? decodeSolAddress(refundAddress) : refundAddress;

  cf.debug(`Registering Liquidity Refund Address ${refundAddress} chain: ${chain} for ${lpuri}`);

  const refundAddressRegisteredEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityProvider.registerLiquidityRefundAddress(encodedAddress(chain, refundAddress)),
    expectedEvent: liquidityProviderLiquidityRefundAddressRegisteredEvent.refine(
      (event) =>
        doAddressesMatch(event.address, chain, refundAddress) && event.accountId === lp.address,
    ),
  });

  cf.debug(
    `Liquidity Refund Address ${refundAddressRegisteredEvent.address} successfully registered chain: ${chain} for ${lpuri}`,
  );
}

export async function depositLiquidity<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
  givenAmount: number,
) {
  const amount = Math.round(givenAmount * 10 ** assetDecimals(ccy)) / 10 ** assetDecimals(ccy);

  const lp = cf.requirements.account.keypair;
  cf.debug(`Depositing ${amount} ${ccy} of liquidity for ${cf.requirements.account.uri}`);

  // If no liquidity refund address is registered, then do that now
  await registerLiquidityRefundAddressForChain(cf, chainFromAsset(ccy), false);

  cf.info(`Opening new ${ccy} liquidity deposit channel for ${lp.address}`);

  const depositAddressReadyEvent = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.liquidityProvider.requestLiquidityDepositAddress(ccy, 0),
    expectedEvent: liquidityProviderLiquidityDepositAddressReadyEvent.refine(
      (event) => event.asset === ccy && event.accountId === lp.address,
    ),
  });
  const ingressAddress = depositAddressReadyEvent.depositAddress.address;

  cf.info(`Initiating transfer of ${ccy} to ${ingressAddress}`);

  const txHash = await runWithTimeout(
    send(cf.logger, ccy, ingressAddress, String(amount)),
    200,
    cf.logger,
    `sending liquidity ${amount} ${ccy}.`,
  );

  const depositFinalisedEvent = await cf.stepUntilEvent(
    ingressEgressDepositFinalisedEvent[chainFromAsset(ccy)].refine(
      (event) =>
        event.channelId === depositAddressReadyEvent.channelId &&
        event.asset === ccy &&
        event.amount === amountToFineAmountBigInt(String(amount), ccy) &&
        event.action.__kind === 'LiquidityProvision' &&
        event.action.lpAccount === lp.address,
    ),
  );

  const amountCredited = depositFinalisedEvent.amount - depositFinalisedEvent.ingressFee;
  const creditedAmount = fineAmountToAmount(amountCredited.toString(), assetDecimals(ccy));
  cf.info(
    `Liquidity deposited to ${ingressAddress} (input amount: ${amount} ${ccy}, credited amount: ${creditedAmount} ${ccy})`,
  );
  return { txHash, amountCredited };
}
