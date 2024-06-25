import assert from 'assert';
import { randomBytes } from 'crypto';
import type { HexString } from '@polkadot/util/types';
import {
  fineAmountToAmount,
  newAddress,
  observeBalanceIncrease,
  assetDecimals,
} from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { fundFlip } from '../shared/fund_flip';
import { redeemFlip, RedeemAmount } from '../shared/redeem_flip';
import { newStatechainAddress } from '../shared/new_statechain_address';
import { getChainflipApi } from './utils/substrate';

// Submitting the `redeem` extrinsic will cost a small amount of gas. Any more than this and we should be suspicious.
const gasErrorMargin = 0.1;

/// Redeems the flip and observed the balance increase
async function redeemAndObserve(
  seed: string,
  redeemEthAddress: HexString,
  redeemAmount: RedeemAmount,
): Promise<number> {
  const initBalance = await getBalance('Flip', redeemEthAddress);
  console.log(`Initial ERC20-Flip balance: ${initBalance}`);

  await redeemFlip(seed, redeemEthAddress, redeemAmount);

  const newBalance = await observeBalanceIncrease('Flip', redeemEthAddress, initBalance);
  const balanceIncrease = newBalance - parseFloat(initBalance);
  console.log(
    `Redemption success! New balance: ${newBalance.toString()}, Increase: ${balanceIncrease}`,
  );

  return balanceIncrease;
}

// Uses the seed to generate a new SC address and Eth address.
// It then funds the SC address with Flip, and redeems the Flip to the Eth address
// checking that the balance has increased the expected amount.
// If no seed is provided, a random one is generated.
export async function testFundRedeem(providedSeed?: string) {
  console.log('=== Starting Fund/Redeem test ===');

  await using chainflip = await getChainflipApi();
  const redemptionTax = await chainflip.query.funding.redemptionTax();
  const redemptionTaxAmount = parseInt(
    fineAmountToAmount(redemptionTax.toString(), assetDecimals('Flip')),
  );
  console.log(`Redemption tax: ${redemptionTax} = ${redemptionTaxAmount} Flip`);

  const seed = providedSeed ?? randomBytes(32).toString('hex');
  const fundAmount = 1000;
  const redeemSCAddress = await newStatechainAddress(seed);
  const redeemEthAddress = await newAddress('Eth', seed);
  console.log(`Flip Redeem address: ${redeemSCAddress}`);
  console.log(`Eth  Redeem address: ${redeemEthAddress}`);

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
  console.log(`Expected balance increase amount: ${exactAmount}`);
  assert.strictEqual(
    redeemedExact.toFixed(5),
    exactAmount.toFixed(5),
    `Unexpected balance increase amount`,
  );
  console.log('Redeem exact amount success!');

  // Test redeeming the rest of the flip with a 'Max' redeem amount
  console.log(`Testing redeem all`);
  const redeemedAll = await redeemAndObserve(seed, redeemEthAddress as HexString, 'Max');
  // We expect to redeem the entire amount minus the exact amount redeemed above + tax & gas for both redemptions
  const expectedRedeemAllAmount = fundAmount - redeemedExact - redemptionTaxAmount * 2;
  assert(
    redeemedAll >= expectedRedeemAllAmount - gasErrorMargin &&
      redeemedAll <= expectedRedeemAllAmount,
    `Unexpected balance increase amount: ${redeemedAll}. Expected between: ${
      expectedRedeemAllAmount - gasErrorMargin
    } - ${expectedRedeemAllAmount}. Did fees change?`,
  );

  console.log('=== Fund/Redeem test complete ===');
}
