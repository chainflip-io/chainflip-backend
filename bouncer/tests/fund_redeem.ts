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

// Uses the seed to generate a new SC address and Eth address.
// It then funds the SC address with Flip, and redeems the Flip to the Eth address
// checking that the balance has increased the expected amount.
// If no seed is provided, a random one is generated.
async function main<A = []>(cf: ChainflipIO<A>, providedSeed?: string) {
  await using chainflip = await getChainflipApi();
  const redemptionTax = await chainflip.query.funding.redemptionTax();
  const redemptionTaxAmount = parseInt(
    fineAmountToAmount(redemptionTax.toString(), assetDecimals('Flip')),
  );
  cf.debug(`Redemption tax: ${redemptionTax} = ${redemptionTaxAmount} Flip`);

  const seed = providedSeed ?? randomBytes(32).toString('hex');
  const fundAmount = 1000;
  const redeemSCAddress = await newStatechainAddress(seed);
  const redeemEthAddress = await newAssetAddress('Eth', seed);
  cf.debug(`Flip Redeem address: ${redeemSCAddress}`);
  cf.debug(`Eth  Redeem address: ${redeemEthAddress}`);

  // Fund the SC address for the tests
  await fundFlip(cf, redeemSCAddress, fundAmount.toString());

  // Test redeeming an exact amount with a portion of the funded flip
  const exactAmount = fundAmount / 4;
  const exactRedeemAmount = { Exact: exactAmount.toString() };
  cf.debug(`Testing redeem exact amount: ${exactRedeemAmount.Exact}`);
  const redeemedExact = await redeemAndObserve(
    cf.logger,
    seed,
    redeemEthAddress as HexString,
    exactRedeemAmount,
  );
  cf.debug(`Expected balance increase amount: ${exactAmount}`);
  assert.strictEqual(
    redeemedExact.toFixed(5),
    exactAmount.toFixed(5),
    `Unexpected balance increase amount`,
  );
  cf.debug('Redeem exact amount success!');

  // Test redeeming the rest of the flip with a 'Max' redeem amount
  cf.debug(`Testing redeem all`);
  const redeemedAll = await redeemAndObserve(cf.logger, seed, redeemEthAddress as HexString, 'Max');
  // We expect to redeem the entire amount minus the exact amount redeemed above + tax & gas for both redemptions
  const expectedRedeemAllAmount = fundAmount - redeemedExact - redemptionTaxAmount;
  assert(
    redeemedAll >= expectedRedeemAllAmount - gasErrorMargin &&
      redeemedAll <= expectedRedeemAllAmount,
    `Unexpected balance increase amount: ${redeemedAll}. Expected between: ${
      expectedRedeemAllAmount - gasErrorMargin
    } - ${expectedRedeemAllAmount}. Did fees change?`,
  );
}

export async function testFundRedeem(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await main(cf, 'redeem');
}
