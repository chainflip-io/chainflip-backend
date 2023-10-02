import assert from 'assert';
import { randomBytes } from 'crypto';
import { HexString } from '@polkadot/util/types';
import { newAddress, observeBalanceIncrease } from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { fundFlip } from '../shared/fund_flip';
import { redeemFlip, RedeemAmount } from '../shared/redeem_flip';
import { newStatechainAddress } from '../shared/new_statechain_address';

const maxRedemptionVariationPercent = 0.03;

/// Redeems the flip and checks that the balance increase is within the variation percent of the expected amount.
async function redeemAndAssertBalanceIncrease(
  seed: string,
  redeemEthAddress: HexString,
  redeemAmount: RedeemAmount,
  expectedBalanceIncrease: number,
): Promise<number> {
  const initBalance = await getBalance('FLIP', redeemEthAddress);
  console.log(`Initial ERC20-FLIP balance: ${initBalance.toString()}`);

  await redeemFlip(seed, redeemEthAddress, redeemAmount);

  const newBalance = await observeBalanceIncrease('FLIP', redeemEthAddress, initBalance);
  const balanceIncrease = newBalance - parseInt(initBalance.toString());
  console.log(
    `Redemption success! New balance: ${newBalance.toString()}, Increase: ${balanceIncrease}`,
  );

  assert(
    Math.abs(balanceIncrease - expectedBalanceIncrease) <
      expectedBalanceIncrease * maxRedemptionVariationPercent,
    `unexpected balance increase: ${balanceIncrease}. Expected: ${expectedBalanceIncrease} +- ${
      maxRedemptionVariationPercent * 100
    }%`,
  );

  return balanceIncrease;
}

// Uses the seed to generate a new SC address and ETH address.
// It then funds the SC address with FLIP, and redeems the FLIP to the ETH address
// checking that the balance has increased the expected amount.
// If no seed is provided, a random one is generated.
export async function testFundRedeem(providedSeed?: string) {
  const seed = providedSeed ?? randomBytes(32).toString('hex');
  const fundAmount = 1000;
  const redeemSCAddress = await newStatechainAddress(seed);
  const redeemEthAddress = await newAddress('ETH', seed);
  console.log(`FLIP Redeem address: ${redeemSCAddress}`);
  console.log(`ETH  Redeem address: ${redeemEthAddress}`);

  // Fund the SC address for the tests
  await fundFlip(redeemSCAddress, fundAmount.toString());

  // Test redeeming an exact amount with a portion of the funded flip
  const exactAmount = fundAmount / 3;
  const exactRedeemAmount = { Exact: exactAmount.toString() };
  console.log(`Testing redeem exact amount: ${exactRedeemAmount.Exact}`);
  const redeemedExact = await redeemAndAssertBalanceIncrease(
    seed,
    redeemEthAddress as HexString,
    exactRedeemAmount,
    exactAmount,
  );

  // Test redeeming the rest of the flip with a 'Max' redeem amount
  console.log(`Testing redeem all`);
  const expectedRedeemAllAmount = fundAmount - redeemedExact;
  await redeemAndAssertBalanceIncrease(
    seed,
    redeemEthAddress as HexString,
    'Max',
    expectedRedeemAllAmount,
  );
}
