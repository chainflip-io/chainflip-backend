import { InternalAsset as Asset } from '@chainflip/cli';
import {
  newAssetAddress,
  decodeDotAddressForContract,
  handleSubstrateError,
  lpMutex,
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
import { Logger } from 'shared/utils/logger';

export async function depositLiquidity(
  parentLogger: Logger,
  ccy: Asset,
  amount: number,
  waitForFinalization = false,
  optionLpUri?: string,
  optionLpMnemonic?: string,
) {
  const lpUri = optionLpUri ?? optionLpMnemonic ?? (process.env.LP_URI || '//LP_1');
  const logger = parentLogger.child({ ccy, amount, lpUri });

  await using chainflip = await getChainflipApi();
  const chain = shortChainFromAsset(ccy);

  const lp = createStateChainKeypair(lpUri, optionLpMnemonic ? true : false);

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

    logger.debug(`Registering Liquidity Refund Address for ${refundAddress}`);
    await lpMutex.runExclusive(lpUri, async () => {
      const nonce = await chainflip.rpc.system.accountNextIndex(lp.address);
      await chainflip.tx.liquidityProvider
        .registerLiquidityRefundAddress({ [chain]: refundAddress })
        .signAndSend(lp, { nonce }, handleSubstrateError(chainflip));
    });
  }

  let eventHandle = observeEvent(logger, 'liquidityProvider:LiquidityDepositAddressReady', {
    test: (event) => event.data.asset === ccy && event.data.accountId === lp.address,
  }).event;

  logger.debug(`Requesting ${ccy} deposit address`);
  await lpMutex.runExclusive(lpUri, async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(lp.address);
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy, null)
      .signAndSend(lp, { nonce }, handleSubstrateError(chainflip));
  });

  const ingressAddress = (await eventHandle).data.depositAddress[chain];

  logger.trace(`Initiating transfer to ${ingressAddress}`);
  eventHandle = observeEvent(logger, 'assetBalances:AccountCredited', {
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
    send(logger, ccy, ingressAddress, String(amount)),
    130,
    logger,
    `sending liquidity ${amount} ${ccy}.`,
  );

  await eventHandle;

  logger.debug(`Liquidity deposited to ${ingressAddress}`);
  return txHash;
}
