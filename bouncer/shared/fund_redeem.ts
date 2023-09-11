import { HexString } from '@polkadot/util/types';
import { newAddress, observeBalanceIncrease, runWithTimeout } from '../shared/utils';
import { getBalance } from '../shared/get_balance';
import { fundFlip } from '../shared/fund_flip';
import { redeemFlip } from '../shared/redeem_flip';
import { newStatechainAddress } from '../shared/new_statechain_address';

// Uses the seed to generate a new SC address and ETH address.
// It then funds the SC address with FLIP, and redeems the FLIP to the ETH address
// checking that the balance has increased.
export async function testFundRedeem(seed: string) {
    const redeemSCAddress = await newStatechainAddress(seed);
    const redeemEthAddress = await newAddress('ETH', seed);
    console.log(`FLIP Redeem address: ${redeemSCAddress}`);
    console.log(`ETH  Redeem address: ${redeemEthAddress}`);
    const initBalance = await getBalance('FLIP', redeemEthAddress);
    console.log(`Initial ERC20-FLIP balance: ${initBalance.toString()}`);
    const amount = 1000;
    // We fund to a specific SC address.
    await fundFlip(redeemSCAddress, amount.toString());

    // The ERC20 FLIP is sent back to an ETH address, and the registered claim can only be executed by that address.
    await redeemFlip(seed, redeemEthAddress as HexString, (amount / 2).toString());
    console.log('Observed RedemptionSettled event');
    const newBalance = await observeBalanceIncrease('FLIP', redeemEthAddress, initBalance);
    console.log(`Redemption success! New balance: ${newBalance.toString()}`);
    console.log('=== Fund/Redeem Test Success ===')
    process.exit(0);
}