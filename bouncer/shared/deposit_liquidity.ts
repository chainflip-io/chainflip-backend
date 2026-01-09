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
import { ChainflipIO, WithLpAccount } from './utils/chainflip_io';

export async function depositLiquidity<A extends WithLpAccount>(
  parentcf: ChainflipIO<A>,
  ccy: Asset,
  givenAmount: number,
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  waitForFinalization = false,
  // optionLpUri?: string,
) {
  const amount = Math.round(givenAmount * 10 ** assetDecimals(ccy)) / 10 ** assetDecimals(ccy);

  const lpUri = parentcf.requirements.account.uri;
  // const lpUri = optionLpUri ?? (process.env.LP_URI || '//LP_1');
  const cf = parentcf.withChildLogger(`${JSON.stringify({ ccy, amount, lpUri })}`);

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

    cf.debug(`Registering Liquidity Refund Address for ${refundAddress}`);
    await cfMutex.runExclusive(lpUri, async () => {
      const nonce = await chainflip.rpc.system.accountNextIndex(lp.address);
      await chainflip.tx.liquidityProvider
        .registerLiquidityRefundAddress({ [chain]: refundAddress })
        .signAndSend(lp, { nonce }, handleSubstrateError(chainflip));
    });
  }

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

  cf.trace(`Initiating transfer to ${ingressAddress}`);

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
