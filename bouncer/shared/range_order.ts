import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import {
  observeEvent,
  getChainflipApi,
  handleSubstrateError,
  assetToDecimals,
  amountToFineAmount,
  lpMutex,
} from '../shared/utils';
import { Asset } from '@chainflip-io/cli';

export async function rangeOrder(ccy: Asset, amount: number) {
  const fine_amount = amountToFineAmount(String(amount), assetToDecimals.get(ccy)!);
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lp_uri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lp_uri);

  const current_sqrt_price = (
    await chainflip.query.liquidityPools.pools(ccy.toLowerCase())
  ).toJSON()!.poolState.rangeOrders.currentSqrtPrice;
  const liquidity = BigInt(
    Math.round((current_sqrt_price / Math.pow(2, 96)) * Number(fine_amount)),
  );
  console.log('Setting up ' + ccy + ' range order');
  const event = observeEvent('liquidityPools:RangeOrderMinted', chainflip, (data) => {
    return data[0] == lp.address && data[1].toUpperCase() == ccy;
  });
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .collectAndMintRangeOrder(ccy.toLowerCase(), [-887272, 887272], { Liquidity: liquidity })
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await event;
}
