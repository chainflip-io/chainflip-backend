#!/usr/bin/env -S pnpm tsx
import { HexString } from '@polkadot/util/types';
import { newAddress, observeBalanceIncrease, runWithTimeout } from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { fundFlip } from '../shared/fund_flip';
import { redeemFlip } from '../shared/redeem_flip';
import { newStatechainAddress } from '../shared/new_statechain_address';

export async function fundRedeemTest() {
  const seed = 'redeem';
  const redeemFlipAddress = await newStatechainAddress(seed);
  const redeemEthAddress = await newAddress('ETH', seed);
  console.log(`FLIP Redeem address: ${redeemFlipAddress}`);
  console.log(`ETH  Redeem address: ${redeemEthAddress}`);
  const initBalance = await getBalance('FLIP', redeemEthAddress);
  console.log(`Initial ERC20-FLIP balance: ${initBalance.toString()}`);
  const amount = 1000;
  await fundFlip(redeemFlipAddress, amount.toString());
  await redeemFlip(seed, redeemEthAddress as HexString, (amount / 2).toString());
  console.log('Observed RedemptionSettled event');
  const newBalance = await observeBalanceIncrease('FLIP', redeemEthAddress, initBalance);
  console.log(`Redemption success! New balance: ${newBalance.toString()}`);
  process.exit(0);
}

runWithTimeout(fundRedeemTest(), 600000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
