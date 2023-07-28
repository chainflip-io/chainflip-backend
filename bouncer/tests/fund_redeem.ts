#!/usr/bin/env -S pnpm tsx
import { HexString } from '@polkadot/util/types';
import { getAddress, observeBalanceIncrease, runWithTimeout } from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { fundFlip } from '../shared/fund_flip';
import { redeemFlip } from '../shared/redeem_flip';

export async function fundRedeemTest() {
  const redeemFlipAddress = await getAddress('FLIP', 'redeem');
  const redeemEthAddress = await getAddress('ETH', 'redeem');
  console.log(`FLIP Redeem address: ${redeemFlipAddress}`);
  console.log(`ETH  Redeem address: ${redeemEthAddress}`);
  const initBalance = await getBalance('FLIP', redeemEthAddress);
  console.log(`Initial ERC20-FLIP balance: ${initBalance.toString()}`);
  const amount = 1000;
  await fundFlip(redeemFlipAddress, amount.toString());
  await redeemFlip('redeem', redeemEthAddress as HexString, (amount / 2).toString());
  console.log('Observed RedemptionSettled event');
  const newBalance = await observeBalanceIncrease('FLIP', redeemEthAddress, initBalance);
  console.log(`Redemption success! New balance: ${newBalance.toString()}`);
  process.exit(0);
}

runWithTimeout(fundRedeemTest(), 600000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
