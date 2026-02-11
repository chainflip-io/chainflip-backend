import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity, registerLiquidityRefundAddressForAsset } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { Asset, chainGasAsset } from 'shared/utils';
import { ChainflipIO, fullAccountFromUri } from 'shared/utils/chainflip_io';

export const deposits = new Map<Asset, number>([
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Btc', 100],
  ['Usdc', 10000000],
  ['ArbUsdc', 1000000],
  ['ArbUsdt', 1000000],
  ['Usdt', 1000000],
  ['Wbtc', 100],
  ['Flip', 100000],
  ['Sol', 1000],
  ['SolUsdc', 100000],
  ['SolUsdt', 100000],
  ['HubDot', 10000],
  ['HubUsdc', 250000],
  ['HubUsdt', 250000],
]);

export const price = new Map<Asset, number>([
  ['Eth', 1000],
  ['ArbEth', 1000],
  ['Btc', 10000],
  ['Usdc', 1],
  ['Usdt', 1],
  ['Wbtc', 10000],
  ['ArbUsdc', 1],
  ['ArbUsdt', 1],
  ['Flip', 10],
  ['Sol', 100],
  ['SolUsdc', 1],
  ['SolUsdt', 1],
  ['HubDot', 10],
  ['HubUsdc', 1],
  ['HubUsdt', 1],
]);

export async function setupSwaps2<A = []>(cf: ChainflipIO<A>): Promise<void> {
  const lp1RefundAddresses = (parentCf: ChainflipIO<A>) =>
    parentCf.with({ account: fullAccountFromUri('//LP_1', 'LP') }).all([
      // (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Ethereum')),
      // (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Bitcoin')),
      //(subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Arbitrum')),
      (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Solana')),
      //(subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Assethub')),
    ]);

  await cf.all([lp1RefundAddresses]);
}

export async function setupSwaps<A = []>(cf: ChainflipIO<A>): Promise<void> {
  cf.info('Setting up for swaps');

  await Promise.all([
    createLpPool(cf.logger, 'Eth', price.get('Eth')!),
    createLpPool(cf.logger, 'Btc', price.get('Btc')!),
    createLpPool(cf.logger, 'Flip', price.get('Flip')!),
    createLpPool(cf.logger, 'Usdt', price.get('Usdt')!),
    createLpPool(cf.logger, 'Wbtc', price.get('Wbtc')!),
    createLpPool(cf.logger, 'ArbEth', price.get('ArbEth')!),
    createLpPool(cf.logger, 'ArbUsdc', price.get('ArbUsdc')!),
    createLpPool(cf.logger, 'ArbUsdt', price.get('ArbUsdt')!),
    createLpPool(cf.logger, 'Sol', price.get('Sol')!),
    createLpPool(cf.logger, 'SolUsdc', price.get('SolUsdc')!),
    createLpPool(cf.logger, 'SolUsdt', price.get('SolUsdt')!),
    createLpPool(cf.logger, 'HubDot', price.get('HubDot')!),
    createLpPool(cf.logger, 'HubUsdc', price.get('HubUsdc')!),
    createLpPool(cf.logger, 'HubUsdt', price.get('HubUsdt')!),
  ]);

  const lp1RefundAddresses = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Ethereum')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Bitcoin')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Arbitrum')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Solana')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Assethub')),
      ]);

  const lpApiRefundAddresses = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_API', 'LP') })
      .all([
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Ethereum')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Bitcoin')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Arbitrum')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Solana')),
        (subcf) => registerLiquidityRefundAddressForAsset(subcf, chainGasAsset('Assethub')),
      ]);

  cf.info('Registering refund addresses');
  await cf.all([lp1RefundAddresses, lpApiRefundAddresses]);

  const lp1Deposits = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => depositLiquidity(subcf, 'Usdc', deposits.get('Usdc')!),
        (subcf) => depositLiquidity(subcf, 'Eth', deposits.get('Eth')!),
        (subcf) => depositLiquidity(subcf, 'Btc', deposits.get('Btc')!),
        (subcf) => depositLiquidity(subcf, 'Flip', deposits.get('Flip')!),
        (subcf) => depositLiquidity(subcf, 'Usdt', deposits.get('Usdt')!),
        (subcf) => depositLiquidity(subcf, 'Wbtc', deposits.get('Wbtc')!),
        (subcf) => depositLiquidity(subcf, 'ArbEth', deposits.get('ArbEth')!),
        (subcf) => depositLiquidity(subcf, 'ArbUsdc', deposits.get('ArbUsdc')!),
        (subcf) => depositLiquidity(subcf, 'ArbUsdt', deposits.get('ArbUsdt')!),
        (subcf) => depositLiquidity(subcf, 'Sol', deposits.get('Sol')!),
        (subcf) => depositLiquidity(subcf, 'SolUsdc', deposits.get('SolUsdc')!),
        (subcf) => depositLiquidity(subcf, 'SolUsdt', deposits.get('SolUsdt')!),
        (subcf) => depositLiquidity(subcf, 'HubDot', deposits.get('HubDot')!),
        (subcf) => depositLiquidity(subcf, 'HubUsdc', deposits.get('HubUsdc')!),
        (subcf) => depositLiquidity(subcf, 'HubUsdt', deposits.get('HubUsdt')!),
      ]);

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
        (subcf) => depositLiquidity(subcf, 'ArbEth', 10),
        (subcf) => depositLiquidity(subcf, 'ArbUsdc', 1000),
        (subcf) => depositLiquidity(subcf, 'ArbUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'Sol', 500),
        (subcf) => depositLiquidity(subcf, 'SolUsdc', 1000),
        (subcf) => depositLiquidity(subcf, 'SolUsdt', 1000),
        (subcf) => depositLiquidity(subcf, 'HubDot', 2000),
        (subcf) => depositLiquidity(subcf, 'HubUsdc', 1000),
        (subcf) => depositLiquidity(subcf, 'HubUsdt', 1000),
      ]);

  cf.info('Depositing liquidity');
  await cf.all([lpApiDeposits, lp1Deposits]);

  const lp1RangeOrders = (parentCf: ChainflipIO<A>) =>
    parentCf
      .with({ account: fullAccountFromUri('//LP_1', 'LP') })
      .all([
        (subcf) => rangeOrder(subcf, 'Eth', deposits.get('Eth')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Btc', deposits.get('Btc')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Eth', deposits.get('Eth')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Btc', deposits.get('Btc')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Flip', deposits.get('Flip')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Usdt', deposits.get('Usdt')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Wbtc', deposits.get('Wbtc')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'ArbEth', deposits.get('ArbEth')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'ArbUsdc', deposits.get('ArbUsdc')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'ArbUsdt', deposits.get('ArbUsdt')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'Sol', deposits.get('Sol')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'SolUsdc', deposits.get('SolUsdc')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'SolUsdt', deposits.get('SolUsdt')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'HubDot', deposits.get('HubDot')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'HubUsdc', deposits.get('HubUsdc')! * 0.9999),
        (subcf) => rangeOrder(subcf, 'HubUsdt', deposits.get('HubUsdt')! * 0.9999),
      ]);

  cf.info('Setting up range orders');
  await cf.all([lp1RangeOrders]);

  cf.debug('Range orders placed');

  cf.info('Swaps Setup completed');
}
