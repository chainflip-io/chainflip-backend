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
  chainGasAsset,
  Chain,
  Asset,
} from 'shared/utils';
import { send } from 'shared/send';
import { getChainflipApi } from 'shared/utils/substrate';
import { liquidityProviderLiquidityDepositAddressReady } from 'generated/events/liquidityProvider/liquidityDepositAddressReady';
import { assetBalancesAccountCredited } from 'generated/events/assetBalances/accountCredited';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';
import { liquidityProviderLiquidityRefundAddressRegistered } from 'generated/events/liquidityProvider/liquidityRefundAddressRegistered';

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
    const currentRefundAddress = (
      await chainflip.query.liquidityProvider.liquidityRefundAddress(lp.address, chain)
    ).toJSON();
    if (currentRefundAddress !== null) {
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
  cf.debug(`Depositing ${amount}${ccy} of liquidity for ${cf.requirements.account.uri}`);

  // If no liquidity refund address is registered, then do that now
  await registerLiquidityRefundAddressForChain(cf, chainFromAsset(ccy), false);

  cf.info(`Opening new liquidity deposit channel for ${lp.address}`);

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

  cf.info(`Initiating transfer to ${ingressAddress}`);

  const txHash = await runWithTimeout(
    send(cf.logger, ccy, ingressAddress, String(amount)),
    200,
    cf.logger,
    `sending liquidity ${amount} ${ccy}.`,
  );

  await cf.stepUntilEvent(
    'AssetBalances.AccountCredited',
    assetBalancesAccountCredited.refine((event) => {
      if (event.asset === ccy && event.accountId === lp.address) {
        if (
          isWithinOnePercent(
            event.amountCredited,
            BigInt(amountToFineAmount(String(amount), assetDecimals(ccy))),
          )
        ) {
          return true;
        }
        cf.info(
          `Received amount ${event.amountCredited} is not within 1% of expected amount ${amountToFineAmount(String(amount), assetDecimals(ccy))} for asset ${ccy}.`,
        );
        return false;
      }
      return false;
    }),
  );

  cf.info(`Liquidity deposited to ${ingressAddress}`);
  return txHash;
}
