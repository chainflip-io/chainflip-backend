import { InternalAsset } from '@chainflip/cli';
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

export async function depositLiquidityViaTransfer(
  parentLogger: Logger,
  ccy: InternalAsset,
  amount: number,
  waitForFinalization = false,
  funderLpMnemonic: string,
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

  const funderLpKeypair = createStateChainKeypair(lpUri, true);

  console.log(`Depositing ${amount} ${ccy} to LP account ${funderLpKeypair.address}`);
  await lpMutex.runExclusive(funderLpMnemonic, async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(funderLpKeypair.address);
    await chainflip.tx.liquidityProvider
      .transferAsset(
        amountToFineAmount(String(amount), assetDecimals(ccy as InternalAsset)),
        ccy as InternalAsset,
        lp.address,
      )
      .signAndSend(funderLpKeypair, { nonce }, handleSubstrateError(chainflip));
  });

  await observeEvent(logger, 'assetBalances:AccountCredited', {
    test: (event) => event.data.asset === ccy && event.data.accountId === lp.address,
    finalized: false,
    timeoutSeconds: 120,
  }).event;

  return;
}
