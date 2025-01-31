import assert from 'assert';
import { InternalAssets as Assets, executeRedemption, getRedemptionDelay } from '@chainflip/cli';
import type { HexString } from '@polkadot/util/types';
import { Wallet, ethers } from 'ethers';
import { getNextEvmNonce } from './send_evm';
import { getGatewayAbi } from './contract_interfaces';
import {
  sleep,
  handleSubstrateError,
  getContractAddress,
  amountToFineAmount,
  observeEVMEvent,
  chainFromAsset,
  getEvmEndpoint,
  assetDecimals,
  createStateChainKeypair,
} from './utils';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { WhaleKeyManager } from './utils/whale_key_manager';

export type RedeemAmount = 'Max' | { Exact: string };

function intoFineAmount(amount: RedeemAmount): RedeemAmount {
  if (typeof amount === 'object' && amount.Exact) {
    const fineAmount = amountToFineAmount(amount.Exact, assetDecimals('Flip'));
    return { Exact: fineAmount };
  }
  return amount;
}

const gatewayAbi = await getGatewayAbi();

export async function redeemFlip(
  flipSeed: string,
  ethAddress: HexString,
  flipAmount: RedeemAmount,
): Promise<string> {
  await using chainflip = await getChainflipApi();
  const flipWallet = createStateChainKeypair('//' + flipSeed);
  const accountIdHex: HexString = `0x${Buffer.from(flipWallet.publicKey).toString('hex')}`;
  const whaleKey = await WhaleKeyManager.getNextKey();
  const ethWallet = new Wallet(whaleKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  const networkOptions = {
    signer: ethWallet,
    network: 'localnet',
    stateChainGatewayContractAddress: getContractAddress('Ethereum', 'GATEWAY'),
    flipContractAddress: getContractAddress('Ethereum', 'Flip'),
  } as const;

  const pendingRedemption = await chainflip.query.flip.pendingRedemptionsReserve(
    flipWallet.publicKey,
  );
  // If a redemption is already in progress, the request will fail.
  assert(
    pendingRedemption.toString().length === 0,
    `A redemption is already in progress for this account: ${accountIdHex}, amount: ${pendingRedemption}`,
  );

  console.log('Requesting redemption');
  const redemptionRequestHandle = observeEvent('funding:RedemptionRequested', {
    test: (event) => event.data.accountId === flipWallet.address,
  }).event;
  const flipperinoRedeemAmount = intoFineAmount(flipAmount);
  await chainflip.tx.funding
    .redeem(flipperinoRedeemAmount, ethAddress, null)
    .signAndSend(flipWallet, { nonce: -1 }, handleSubstrateError(chainflip));

  const redemptionRequestEvent = await redemptionRequestHandle;
  console.log('Redemption requested: ', redemptionRequestEvent.data.amount);

  console.log('Waiting for redemption to be registered');
  const observeEventAmount = flipperinoRedeemAmount === 'Max' ? '*' : flipperinoRedeemAmount.Exact;
  await observeEVMEvent(
    chainFromAsset(Assets.Flip),
    gatewayAbi,
    getContractAddress('Ethereum', 'GATEWAY'),
    'RedemptionRegistered',
    [accountIdHex, observeEventAmount, ethAddress, '*', '*', '*'],
  );

  const delay = await getRedemptionDelay(networkOptions);
  console.log(`Waiting for ${delay}s before we can execute redemption`);
  await sleep(Number(delay) * 1000);

  console.log(`Executing redemption`);

  const nonce = await getNextEvmNonce('Ethereum', whaleKey);

  const redemptionExecutedHandle = observeEvent('funding:RedemptionSettled', {
    test: (event) => event.data[0] === flipWallet.address,
  }).event;

  await executeRedemption(accountIdHex, networkOptions, { nonce });
  const redemptionExecutedAmount = (await redemptionExecutedHandle).data[1];
  console.log('Observed RedemptionSettled event: ', redemptionExecutedAmount);
  assert.strictEqual(
    redemptionExecutedAmount,
    redemptionRequestEvent.data.amount,
    "RedemptionSettled amount doesn't match RedemptionRequested amount",
  );

  return redemptionExecutedAmount;
}
