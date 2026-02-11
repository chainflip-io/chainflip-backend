import { InternalAsset as Asset } from '@chainflip/cli';
import {
  newAssetAddress,
  decodeDotAddressForContract,
  amountToFineAmount,
  isWithinOnePercent,
  chainFromAsset,
  decodeSolAddress,
  assetDecimals,
  runWithTimeout,
  shortChainFromChain,
  doAddressesMatch,
} from 'shared/utils';
import { send } from 'shared/send';
import { getChainflipApi } from 'shared/utils/substrate';
import { liquidityProviderLiquidityDepositAddressReady } from 'generated/events/liquidityProvider/liquidityDepositAddressReady';
import { assetBalancesAccountCredited } from 'generated/events/assetBalances/accountCredited';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';
import { liquidityProviderLiquidityRefundAddressRegistered } from 'generated/events/liquidityProvider/liquidityRefundAddressRegistered';

export async function registerLiquidityRefundAddressForAsset<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
) {
  const lpuri = cf.requirements.account.uri;
  const lp = cf.requirements.account.keypair;

  let refundAddress = await newAssetAddress(ccy, lpuri);
  const chain = chainFromAsset(ccy);

  refundAddress = chain === 'Assethub' ? decodeDotAddressForContract(refundAddress) : refundAddress;
  refundAddress = chain === 'Solana' ? decodeSolAddress(refundAddress) : refundAddress;

  cf.debug(`Registering Liquidity Refund Address ${refundAddress} asset: ${ccy} for ${lpuri}`);

  const refundAddressRegisteredEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.liquidityProvider.registerLiquidityRefundAddress({
        [shortChainFromChain(chain)]: refundAddress,
      }),
    expectedEvent: {
      name: 'LiquidityProvider.LiquidityRefundAddressRegistered',
      schema: liquidityProviderLiquidityRefundAddressRegistered.refine(
        (event) =>
          doAddressesMatch(event.address, chain, refundAddress) && event.accountId === lp.address,
      ),
    },
  });

  cf.debug(
    `Liquidity Refund Address ${refundAddressRegisteredEvent.address} successfully registered asset: ${ccy} for ${lpuri}`,
  );
}

export async function depositLiquidity<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
  givenAmount: number,
) {
  const amount = Math.round(givenAmount * 10 ** assetDecimals(ccy)) / 10 ** assetDecimals(ccy);

  const lp = cf.requirements.account.keypair;
  cf.debug(`Depositing ${amount}${ccy} of liquidity for ${cf.requirements.account.uri}`);

  await using chainflip = await getChainflipApi();
  const chain = chainFromAsset(ccy);

  // If no liquidity refund address is registered, then do that now
  if (
    (await chainflip.query.liquidityProvider.liquidityRefundAddress(lp.address, chain)).toJSON() ===
    null
  ) {
    await registerLiquidityRefundAddressForAsset(cf, ccy);
  }

  cf.debug(`Opening new liquidity deposit channel for ${lp.address}`);

  const depositAddressReadyEvent = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.liquidityProvider.requestLiquidityDepositAddress(ccy, null),
    expectedEvent: {
      name: 'LiquidityProvider.LiquidityDepositAddressReady',
      schema: liquidityProviderLiquidityDepositAddressReady.refine(
        (event) => event.asset === ccy && event.accountId === lp.address,
      ),
    },
  });
  const ingressAddress = depositAddressReadyEvent.depositAddress.address;

  cf.debug(`Initiating transfer to ${ingressAddress}`);

  const txHash = await runWithTimeout(
    send(cf.logger, ccy, ingressAddress, String(amount)),
    130,
    cf.logger,
    `sending liquidity ${amount} ${ccy}.`,
  );

  await cf.stepUntilEvent(
    'AssetBalances.AccountCredited',
    assetBalancesAccountCredited.refine(
      (event) =>
        event.asset === ccy &&
        event.accountId === lp.address &&
        isWithinOnePercent(
          event.amountCredited,
          BigInt(amountToFineAmount(String(amount), assetDecimals(ccy))),
        ),
    ),
  );

  cf.debug(`Liquidity deposited to ${ingressAddress}`);
  return txHash;
}
