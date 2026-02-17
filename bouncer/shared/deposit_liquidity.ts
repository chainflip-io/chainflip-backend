import { InternalAsset as Asset } from '@chainflip/cli';
import {
  newAssetAddress,
  decodeDotAddressForContract,
  handleSubstrateError,
  cfMutex,
  shortChainFromAsset,
  amountToFineAmount,
  isWithinOnePercent,
  chainFromAsset,
  decodeSolAddress,
  assetDecimals,
  createStateChainKeypair,
  runWithTimeout,
} from 'shared/utils';
import { send } from 'shared/send';
import { getChainflipApi } from 'shared/utils/substrate';
import { liquidityProviderLiquidityDepositAddressReady } from 'generated/events/liquidityProvider/liquidityDepositAddressReady';
import { assetBalancesAccountCredited } from 'generated/events/assetBalances/accountCredited';
import { ChainflipIO, WithLpAccount } from 'shared/utils/chainflip_io';

export async function depositLiquidity<A extends WithLpAccount>(
  cf: ChainflipIO<A>,
  ccy: Asset,
  givenAmount: number,
) {
  const amount = Math.round(givenAmount * 10 ** assetDecimals(ccy)) / 10 ** assetDecimals(ccy);

  const lpUri = cf.requirements.account.uri;
  cf.info(`Depositing ${amount}${ccy} of liquidity for ${lpUri}`);

  await using chainflip = await getChainflipApi();
  const chain = shortChainFromAsset(ccy);

  const lp = createStateChainKeypair(lpUri);

  // If no liquidity refund address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.liquidityRefundAddress(
        lp.address,
        chainFromAsset(ccy),
      )
    ).toJSON() === null
  ) {
    let refundAddress = await newAssetAddress(ccy, 'LP_1');
    refundAddress = chain === 'Hub' ? decodeDotAddressForContract(refundAddress) : refundAddress;
    refundAddress = chain === 'Sol' ? decodeSolAddress(refundAddress) : refundAddress;

    cf.info(`Registering Liquidity Refund Address for ${refundAddress}`);
    await cfMutex.runExclusive(lpUri, async () => {
      const nonce = await chainflip.rpc.system.accountNextIndex(lp.address);
      await chainflip.tx.liquidityProvider
        .registerLiquidityRefundAddress({ [chain]: refundAddress })
        .signAndSend(lp, { nonce }, handleSubstrateError(chainflip));
    });
  }

  cf.info(`Opening new liquidity deposit channel for ${lpUri}`);

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
    130,
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
        } else {
          cf.info(
            `Received amount ${event.amountCredited} is not within 1% of expected amount ${amountToFineAmount(String(amount), assetDecimals(ccy))}.`,
          );
          return false;
        }
      } else {
        return false;
      }
    }),
  );

  cf.info(`Liquidity deposited to ${ingressAddress}`);
  return txHash;
}
