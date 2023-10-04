import assert from 'assert';
import { randomBytes } from 'crypto';
import { HexString } from '@polkadot/util/types';
import { assetDecimals } from '@chainflip-io/cli';
import {
  fineAmountToAmount,
  newAddress,
  observeBalanceIncrease,
  getChainflipApi,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { fundFlip } from '../shared/fund_flip';
import { redeemFlip, RedeemAmount } from '../shared/redeem_flip';
import { newStatechainAddress } from '../shared/new_statechain_address';

const expectedFeesFlip = 0.000025;

/// Redeems the flip and observed the balance increase
async function redeemAndObserve(
  seed: string,
  redeemEthAddress: HexString,
  redeemAmount: RedeemAmount,
): Promise<number> {
  const initBalance = await getBalance('FLIP', redeemEthAddress);
  console.log(`Initial ERC20-FLIP balance: ${initBalance}`);

  await redeemFlip(seed, redeemEthAddress, redeemAmount);

  const newBalance = await observeBalanceIncrease('FLIP', redeemEthAddress, initBalance);
  const balanceIncrease = newBalance - parseInt(initBalance);
  console.log(
    `Redemption success! New balance: ${newBalance.toString()}, Increase: ${balanceIncrease}`,
  );

  return balanceIncrease;
}

// Uses the seed to generate a new SC address and ETH address.
// It then funds the SC address with FLIP, and redeems the FLIP to the ETH address
// checking that the balance has increased the expected amount.
// If no seed is provided, a random one is generated.
export async function testFundRedeem(providedSeed?: string) {
  const chainflip = await getChainflipApi();
  const redemptionTax = await chainflip.query.funding.redemptionTax();
  const redemptionTaxAmount = parseInt(
    fineAmountToAmount(redemptionTax.toString(), assetDecimals.FLIP),
  );
  console.log(`Redemption tax: ${redemptionTax} = ${redemptionTaxAmount} FLIP`);

  const seed = providedSeed ?? randomBytes(32).toString('hex');
  const fundAmount = 1000;
  const redeemSCAddress = await newStatechainAddress(seed);
  const redeemEthAddress = await newAddress('ETH', seed);
  console.log(`FLIP Redeem address: ${redeemSCAddress}`);
  console.log(`ETH  Redeem address: ${redeemEthAddress}`);

  // Fund the SC address for the tests
  await fundFlip(redeemSCAddress, fundAmount.toString());

  // Test redeeming an exact amount with a portion of the funded flip
  const exactAmount = fundAmount / 4;
  const exactRedeemAmount = { Exact: exactAmount.toString() };
  console.log(`Testing redeem exact amount: ${exactRedeemAmount.Exact}`);
  const redeemedExact = await redeemAndObserve(
    seed,
    redeemEthAddress as HexString,
    exactRedeemAmount,
  );
  assert.strictEqual(redeemedExact, exactAmount, `Unexpected balance increase amount`);

  // Test redeeming the rest of the flip with a 'Max' redeem amount
  console.log(`Testing redeem all`);
  const redeemedAll = await redeemAndObserve(seed, redeemEthAddress as HexString, 'Max');
  // We expect to redeem the entire amount minus the exact amount redeemed above + tax & fees for both redemptions
  const expectedRedeemAllAmount = fundAmount - redeemedExact - redemptionTaxAmount * 2;
  assert(
    redeemedAll >= expectedRedeemAllAmount - expectedFeesFlip &&
      redeemedAll <= expectedRedeemAllAmount,
    `Unexpected balance increase amount: ${redeemedAll}. Expected between: ${
      expectedRedeemAllAmount - expectedFeesFlip
    } - ${expectedRedeemAllAmount}. Did fees change?`,
  );
}
