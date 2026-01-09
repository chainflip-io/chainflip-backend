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
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { ChainflipIO } from './utils/chainflip_io';

export async function depositLiquidity<A = []>(
  parentcf: ChainflipIO<A>,
  ccy: Asset,
  givenAmount: number,
  waitForFinalization = false,
  optionLpUri?: string,
) {
  const amount = Math.round(givenAmount * 10 ** assetDecimals(ccy)) / 10 ** assetDecimals(ccy);

  const lpUri = optionLpUri ?? (process.env.LP_URI || '//LP_1');
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

  let eventHandle = observeEvent(cf.logger, 'liquidityProvider:LiquidityDepositAddressReady', {
    test: (event) => event.data.asset === ccy && event.data.accountId === lp.address,
  }).event;

  cf.debug(`Requesting ${ccy} deposit address`);
  await cfMutex.runExclusive(lpUri, async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(lp.address);
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy, null)
      .signAndSend(lp, { nonce }, handleSubstrateError(chainflip));
  });

  const ingressAddress = (await eventHandle).data.depositAddress[chain];

  cf.trace(`Initiating transfer to ${ingressAddress}`);
  eventHandle = observeEvent(cf.logger, 'assetBalances:AccountCredited', {
    test: (event) =>
      event.data.asset === ccy &&
      event.data.accountId === lp.address &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(amountToFineAmount(String(amount), assetDecimals(ccy))),
      ),
    finalized: waitForFinalization,
    timeoutSeconds: 120,
  }).event;

  const txHash = await runWithTimeout(
    send(cf.logger, ccy, ingressAddress, String(amount)),
    130,
    cf.logger,
    `sending liquidity ${amount} ${ccy}.`,
  );

  await eventHandle;

  cf.debug(`Liquidity deposited to ${ingressAddress}`);
  return txHash;
}
