import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity, registerLiquidityRefundAddressForChain } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { assetDecimals, Asset, fineAmountToAmount } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';

export const deposits = new Map<Asset, number>([
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Bnb', 1000],
  ['Btc', 25],
  ['Usdc', 15000000],
  ['ArbUsdc', 1000000],
  ['ArbUsdt', 1000000],
  ['BscUsdt', 1000000],
  ['Usdt', 1000000],
  ['Wbtc', 25],
  ['Cbbtc', 25],
  ['Flip', 100000],
  ['Sol', 1000],
  ['SolUsdc', 100000],
  ['SolUsdt', 100000],
  ['Trx', 1000000],
  ['TrxUsdt', 100000],
  ['HubDot', 10000],
  ['HubUsdc', 250000],
  ['HubUsdt', 250000],
]);

// Also pushed to the mock Chainlink feeds by `updateDefaultPriceFeeds`, so the oracle and the
// pools always agree.
//
// Two constraints when changing these:
//  - For assets with no oracle feed (see `get_chainlink_assetpair`), fees are estimated by
//    probing the pool, and the runtime discards a probe that deviates from
//    `hard_coded_price_for_asset` by more than `MAX_PRICE_ESTIMATE_DEVIATION_FACTOR`. A price
//    outside that band makes every fee denominated in that asset wrong by the full ratio.
//  - Each full-range order consumes roughly `deposits * price` of the Usdc deposit, so the sum
//    over all pools has to stay under `deposits.get('Usdc')`.
export const price = new Map<Asset, number>([
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Bnb', 600],
  ['Btc', 40000],
  ['Usdc', 1],
  ['Usdt', 1],
  ['Wbtc', 40000],
  ['Cbbtc', 40000],
  ['ArbUsdc', 1],
  ['ArbUsdt', 1],
  ['BscUsdt', 1],
  ['Flip', 1],
  ['Sol', 100],
  ['SolUsdc', 1],
  ['SolUsdt', 1],
  ['Trx', 1],
  ['TrxUsdt', 1],
  ['HubDot', 2],
  ['HubUsdc', 1],
  ['HubUsdt', 1],
]);

export async function setupSwaps<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up for swaps');

  await Promise.all([
    createLpPool(cf.logger, 'Eth', price.get('Eth')!),
    createLpPool(cf.logger, 'Btc', price.get('Btc')!),
    createLpPool(cf.logger, 'Flip', price.get('Flip')!),
    createLpPool(cf.logger, 'Usdt', price.get('Usdt')!),
    createLpPool(cf.logger, 'Wbtc', price.get('Wbtc')!),
    createLpPool(cf.logger, 'Cbbtc', price.get('Cbbtc')!),
    createLpPool(cf.logger, 'ArbEth', price.get('ArbEth')!),
    createLpPool(cf.logger, 'ArbUsdc', price.get('ArbUsdc')!),
    createLpPool(cf.logger, 'ArbUsdt', price.get('ArbUsdt')!),
    createLpPool(cf.logger, 'Bnb', price.get('Bnb')!),
    createLpPool(cf.logger, 'BscUsdt', price.get('BscUsdt')!),
    createLpPool(cf.logger, 'Sol', price.get('Sol')!),
    createLpPool(cf.logger, 'SolUsdc', price.get('SolUsdc')!),
    createLpPool(cf.logger, 'SolUsdt', price.get('SolUsdt')!),
    createLpPool(cf.logger, 'Trx', price.get('Trx')!),
    createLpPool(cf.logger, 'TrxUsdt', price.get('Trx')!),
    createLpPool(cf.logger, 'HubDot', price.get('HubDot')!),
    createLpPool(cf.logger, 'HubUsdc', price.get('HubUsdc')!),
    createLpPool(cf.logger, 'HubUsdt', price.get('HubUsdt')!),
  ]);

  // Set permissive default oracle slippage (100%) for all pools to prevent swap failures in tests.
  await submitGovernanceExtrinsic((api) =>
    api.tx.swapping.updatePalletConfig(
      [...price.keys()]
        .filter((a): a is Asset => a !== 'Usdc')
        .map((asset) => ({
          type: 'SetDefaultOraclePriceSlippageProtectionForAsset' as const,
          value: {
            baseAsset: asset,
            quoteAsset: 'Usdc' as const,
            bps: 10000,
          },
        })),
    ),
  );

  cf.info('Registering refund addresses');
  await cf.all(
    ['//LP_1', '//LP_API'].map(
      (uri) => (parentCf) =>
        parentCf
          .with({ account: fullAccountFromUri(uri as `//${string}`, 'LP') })
          .all([
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Ethereum'),
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Bitcoin'),
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Arbitrum'),
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Bsc'),
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Solana'),
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Assethub'),
            (subcf) => registerLiquidityRefundAddressForChain(subcf, 'Tron'),
          ]),
    ),
  );

  const lp1Deposits = async (parentCf: ChainflipIO<A>) => {
    const depositResults = await parentCf.with({ account: fullAccountFromUri('//LP_1', 'LP') }).all(
      [...deposits].map(([asset, amount]) => async (subcf) => ({
        asset,
        ...(await depositLiquidity(subcf, asset, amount)),
      })),
    );

    return new Map(depositResults.map(({ asset, amountCredited }) => [asset, amountCredited]));
  };

  const lpApiDeposits = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_API', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Usdc', 1000),
        (subcf) => depositLiquidity(subcf, 'Eth', 100),
        (subcf) => depositLiquidity(subcf, 'Btc', 10),
        (subcf) => depositLiquidity(subcf, 'Flip', 10000),
        (subcf) => depositLiquidity(subcf, 'Usdt', 1000),
        (subcf) => depositLiquidity(subcf, 'Wbtc', 10),
        (subcf) => depositLiquidity(subcf, 'Cbbtc', 10),
        (subcf) => depositLiquidity(subcf, 'ArbEth', 10),
        (subcf) => depositLiquidity(subcf, 'ArbUsdc', 1000),
        (subcf) => depositLiquidity(subcf, 'ArbUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'Bnb', 10),
        (subcf) => depositLiquidity(subcf, 'BscUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'Sol', 500),
        (subcf) => depositLiquidity(subcf, 'SolUsdc', 1000),
        (subcf) => depositLiquidity(subcf, 'SolUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'Trx', 10000),
        (subcf) => depositLiquidity(subcf, 'TrxUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'HubDot', 2000),
        (subcf) => depositLiquidity(subcf, 'HubUsdc', 1000),
        (subcf) => depositLiquidity(subcf, 'HubUsdt', 1000),
      ]);

  cf.info('Depositing liquidity');
  const [, lp1DepositedAmounts] = await cf.all([lpApiDeposits, lp1Deposits]);

  const rangeOrderAmount = (asset: Asset): string => {
    const amountCredited = lp1DepositedAmounts.get(asset);
    if (amountCredited === undefined) {
      throw new Error(`Missing credited deposit amount for ${asset}`);
    }

    const bufferedAmount = (amountCredited * 9999n) / 10000n;
    return fineAmountToAmount(bufferedAmount.toString(), assetDecimals(asset));
  };

  const lp1RangeOrders = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => rangeOrder(subcf, 'Eth', rangeOrderAmount('Eth')),
        (subcf) => rangeOrder(subcf, 'Btc', rangeOrderAmount('Btc')),
        (subcf) => rangeOrder(subcf, 'Flip', rangeOrderAmount('Flip')),
        (subcf) => rangeOrder(subcf, 'Usdt', rangeOrderAmount('Usdt')),
        (subcf) => rangeOrder(subcf, 'Wbtc', rangeOrderAmount('Wbtc')),
        (subcf) => rangeOrder(subcf, 'Cbbtc', rangeOrderAmount('Cbbtc')),
        (subcf) => rangeOrder(subcf, 'ArbEth', rangeOrderAmount('ArbEth')),
        (subcf) => rangeOrder(subcf, 'ArbUsdc', rangeOrderAmount('ArbUsdc')),
        (subcf) => rangeOrder(subcf, 'ArbUsdt', rangeOrderAmount('ArbUsdt')),
        (subcf) => rangeOrder(subcf, 'Bnb', rangeOrderAmount('Bnb')),
        (subcf) => rangeOrder(subcf, 'BscUsdt', rangeOrderAmount('BscUsdt')),
        (subcf) => rangeOrder(subcf, 'Sol', rangeOrderAmount('Sol')),
        (subcf) => rangeOrder(subcf, 'SolUsdc', rangeOrderAmount('SolUsdc')),
        (subcf) => rangeOrder(subcf, 'SolUsdt', rangeOrderAmount('SolUsdt')),
        (subcf) => rangeOrder(subcf, 'Trx', rangeOrderAmount('Trx')),
        (subcf) => rangeOrder(subcf, 'TrxUsdt', rangeOrderAmount('TrxUsdt')),
        (subcf) => rangeOrder(subcf, 'HubDot', rangeOrderAmount('HubDot')),
        (subcf) => rangeOrder(subcf, 'HubUsdc', rangeOrderAmount('HubUsdc')),
        (subcf) => rangeOrder(subcf, 'HubUsdt', rangeOrderAmount('HubUsdt')),
      ]);

  cf.info('Setting up range orders');
  await cf.all([lp1RangeOrders]);

  cf.debug('Range orders placed');

  cf.info('Swaps Setup completed');
}
