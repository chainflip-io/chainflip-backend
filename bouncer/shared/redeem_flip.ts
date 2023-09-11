import { assetDecimals, executeRedemption, getRedemptionDelay } from '@chainflip-io/cli';
import { HexString } from '@polkadot/util/types';
import { Wallet, ethers } from 'ethers';
import Keyring from '@polkadot/keyring';
import { getNextEthNonce } from './send_eth';
import { getGatewayAbi } from './eth_abis';
import {
  sleep,
  observeEvent,
  handleSubstrateError,
  getEthContractAddress,
  getChainflipApi,
  amountToFineAmount,
  observeEVMEvent,
} from './utils';

const gatewayAbi = await getGatewayAbi();

export async function redeemFlip(flipSeed: string, ethAddress: HexString, flipAmount: string) {
  const chainflip = await getChainflipApi();
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const flipWallet = keyring.createFromUri('//' + flipSeed);
  const accountIdHex = `0x${Buffer.from(flipWallet.publicKey).toString('hex')}`;
  const flipperinoAmount = amountToFineAmount(flipAmount, assetDecimals.FLIP);
  const ethWallet = new Wallet(
    process.env.ETH_USDC_WHALE ??
    '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80',
  ).connect(ethers.getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  console.log('Requesting redemption');
  const redemptionRequestHandle = observeEvent(
    'funding:RedemptionRequested',
    chainflip,
    (event) => event.data.accountId === flipWallet.address,
  );
  await chainflip.tx.funding
    .redeem({ Exact: flipperinoAmount }, ethAddress, null)
    .signAndSend(flipWallet, { nonce: -1 }, handleSubstrateError(chainflip));
  await redemptionRequestHandle;

  const networkOptions = {
    signer: ethWallet,
    network: 'localnet',
    stateChainGatewayContractAddress: getEthContractAddress('GATEWAY'),
    flipContractAddress: getEthContractAddress('FLIP'),
  } as const;
  console.log('Waiting for redemption to be registered');
  await observeEVMEvent(gatewayAbi, getEthContractAddress('GATEWAY'), 'RedemptionRegistered', [
    accountIdHex,
    flipperinoAmount,
    ethAddress,
    '*',
    '*',
    '*',
  ]);

  const delay = await getRedemptionDelay(networkOptions);
  console.log(`Waiting for ${delay}s before we can execute redemption`);
  await sleep(delay * 1000);

  console.log(`Executing redemption`);

  const nonce = await getNextEthNonce();

  const redemptionExecutedHandle = observeEvent(
    'funding:RedemptionSettled',
    chainflip,
    (event) => event.data[0] === flipWallet.address,
  );

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await executeRedemption(accountIdHex as any, networkOptions, { nonce: BigInt(nonce) });
  await redemptionExecutedHandle;
}
