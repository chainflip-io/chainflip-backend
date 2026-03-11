import assert from 'assert';
import { randomBytes } from 'crypto';
import type { HexString } from '@polkadot/util/types';
import {
  fineAmountToAmount,
  newAssetAddress,
  observeBalanceIncrease,
  assetDecimals,
} from 'shared/utils';
import { getBalance } from 'shared/get_balance';
import { fundFlip } from 'shared/fund_flip';
import { redeemFlip, RedeemAmount } from 'shared/redeem_flip';
import { newStatechainAddress } from 'shared/new_statechain_address';
import { getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';

// Submitting the `redeem` extrinsic will cost a small amount of gas. Any more than this and we should be suspicious.
const gasErrorMargin = 0.1;

/// Redeems the flip and observed the balance increase
async function redeemAndObserve(
  logger: Logger,
  seed: string,
  redeemEthAddress: HexString,
  redeemAmount: RedeemAmount,
): Promise<number> {
  const initBalance = await getBalance('Flip', redeemEthAddress);
  logger.debug(`Initial ERC20-Flip balance: ${initBalance}`);

  await redeemFlip(logger, seed, redeemEthAddress, redeemAmount);

  const newBalance = await observeBalanceIncrease(logger, 'Flip', redeemEthAddress, initBalance);
  const balanceIncrease = newBalance - parseFloat(initBalance);
  logger.debug(
    `Redemption success! New balance: ${newBalance.toString()}, Increase: ${balanceIncrease}`,
  );

  return balanceIncrease;
}

// Funds a new account and redeems flip, returning the balance increase.
async function fundAndRedeem<A>(
  cf: ChainflipIO<A>,
  seed: string,
  fundAmount: number,
  redeemAmount: RedeemAmount,
): Promise<number> {
  const scAddress = await newStatechainAddress(seed);
  const ethAddress = await newAssetAddress('Eth', seed);
  cf.debug(`Redeem Flip address: ${scAddress}, Eth address: ${ethAddress}`);

  await fundFlip(cf, scAddress, fundAmount.toString());

  return redeemAndObserve(cf.logger, seed, ethAddress as HexString, redeemAmount);
}

// Runs the exact and max redemption tests in parallel using separate accounts.
async function main<A = []>(cf: ChainflipIO<A>, providedSeed?: string) {
  await using chainflip = await getChainflipApi();
  const redemptionTax = await chainflip.query.funding.redemptionTax();
  const redemptionTaxAmount = parseInt(
    fineAmountToAmount(redemptionTax.toString(), assetDecimals('Flip')),
  );
  cf.debug(`Redemption tax: ${redemptionTax} = ${redemptionTaxAmount} Flip`);

  const baseSeed = providedSeed ?? randomBytes(32).toString('hex');
  const fundAmount = 1000;
  const exactAmount = fundAmount / 4;

  cf.debug(`Testing redeem exact (${exactAmount}) and redeem max in parallel`);
  const [redeemedExact, redeemedAll] = await cf.all([
    (subcf) =>
      fundAndRedeem(subcf, `${baseSeed}_exact`, fundAmount, { Exact: exactAmount.toString() }),
    (subcf) => fundAndRedeem(subcf, `${baseSeed}_max`, fundAmount, 'Max'),
  ]);

  // Verify exact redemption
  assert.strictEqual(
    redeemedExact.toFixed(5),
    exactAmount.toFixed(5),
    `Unexpected balance increase amount for exact redemption`,
  );
  cf.debug('Redeem exact amount success!');

  // Verify max redemption, no redemption tax is applied since the account doesn't have any bonded funds.
  const expectedRedeemAllAmount = fundAmount;
  assert(
    redeemedAll >= expectedRedeemAllAmount - gasErrorMargin &&
      redeemedAll <= expectedRedeemAllAmount,
    `Unexpected balance increase amount for max redemption: ${redeemedAll}. Expected between: ${
      expectedRedeemAllAmount - gasErrorMargin
    } - ${expectedRedeemAllAmount}. Did fees change?`,
  );
  cf.debug('Redeem max amount success!');
}

export async function testFundRedeem(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await main(cf, 'redeem');
}
