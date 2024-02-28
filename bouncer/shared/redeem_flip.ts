import assert from 'assert';
import { Assets, assetDecimals, executeRedemption, getRedemptionDelay } from '@chainflip-io/cli';
import { HexString } from '@polkadot/util/types';
import { Wallet, ethers } from 'ethers';
import Keyring from '@polkadot/keyring';
import { getNextEvmNonce } from './send_evm';
import { getGatewayAbi } from './contract_interfaces';
import {
  sleep,
  observeEvent,
  handleSubstrateError,
  getContractAddress,
  getChainflipApi,
  amountToFineAmount,
  observeEVMEvent,
  chainFromAsset,
  getEvmEndpoint,
  getWhaleKey,
} from './utils';

export type RedeemAmount = 'Max' | { Exact: string };

function intoFineAmount(amount: RedeemAmount): RedeemAmount {
  if (typeof amount === 'object' && amount.Exact) {
    const fineAmount = amountToFineAmount(amount.Exact, assetDecimals.FLIP);
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
  const chainflip = await getChainflipApi();
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const flipWallet = keyring.createFromUri('//' + flipSeed);
  const accountIdHex: HexString = `0x${Buffer.from(flipWallet.publicKey).toString('hex')}`;
  const ethWallet = new Wallet(getWhaleKey('Ethereum')).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  const networkOptions = {
    signer: ethWallet,
    network: 'localnet',
    stateChainGatewayContractAddress: getContractAddress('Ethereum', 'GATEWAY'),
    flipContractAddress: getContractAddress('Ethereum', 'FLIP'),
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
  const redemptionRequestHandle = observeEvent(
    'funding:RedemptionRequested',
    chainflip,
    (event) => event.data.accountId === flipWallet.address,
  );
  const flipperinoRedeemAmount = intoFineAmount(flipAmount);
  await chainflip.tx.funding
    .redeem(flipperinoRedeemAmount, ethAddress, null)
    .signAndSend(flipWallet, { nonce: -1 }, handleSubstrateError(chainflip));

  const redemptionRequestEvent = await redemptionRequestHandle;
  console.log('Redemption requested: ', redemptionRequestEvent.data.amount);

  console.log('Waiting for redemption to be registered');
  const observeEventAmount = flipperinoRedeemAmount === 'Max' ? '*' : flipperinoRedeemAmount.Exact;
  await observeEVMEvent(
    chainFromAsset(Assets.FLIP),
    gatewayAbi,
    getContractAddress('Ethereum', 'GATEWAY'),
    'RedemptionRegistered',
    [accountIdHex, observeEventAmount, ethAddress, '*', '*', '*'],
  );

  const delay = await getRedemptionDelay(networkOptions);
  console.log(`Waiting for ${delay}s before we can execute redemption`);
  await sleep(Number(delay) * 1000);

  console.log(`Executing redemption`);

  const nonce = await getNextEvmNonce('Ethereum');

  const redemptionExecutedHandle = observeEvent(
    'funding:RedemptionSettled',
    chainflip,
    (event) => event.data[0] === flipWallet.address,
  );

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
