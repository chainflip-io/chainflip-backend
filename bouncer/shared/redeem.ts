import { cryptoWaitReady, sr25519PairFromSeed } from '@polkadot/util-crypto';
import fs from 'fs/promises';
import { Wallet, ethers } from 'ethers';
import { KeyringPair } from '@polkadot/keyring/types';
import Keyring from '@polkadot/keyring';
import { executeRedemption, getRedemptionDelay } from '@chainflip-io/cli';
import { Mutex } from 'async-mutex';
import {
  getAddress,
  getChainflipApi,
  observeBalanceIncrease,
  observeEvent,
  sleep,
  handleSubstrateError,
  getEthContractAddress,
} from './utils';
import { getBalance } from './get_balance';
import { getNextEthNonce } from './send_eth';

async function getBashfulSigningKey(): Promise<KeyringPair> {
  await cryptoWaitReady();

  const bashfulKeyHex = await fs.readFile('../localnet/init/secrets/signing_key_file', 'utf-8');
  const bashfulKey = sr25519PairFromSeed(Buffer.from(bashfulKeyHex, 'hex'));
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);

  return keyring.createFromPair(bashfulKey);
}

const bashfulSigningMutex = new Mutex();

export async function redeemTest() {
  await cryptoWaitReady();

  const redeemAddress = await getAddress('FLIP', 'redeem');
  console.log(`Redeem address ${redeemAddress}`);

  const initBalance = await getBalance('FLIP', redeemAddress);

  console.log(`Initial FLIP balance: ${initBalance.toString()}`);

  const chainflip = await getChainflipApi();
  const bashful = await getBashfulSigningKey();

  const amount = 1000;

  await bashfulSigningMutex.runExclusive(async () => {
    chainflip.tx.funding
      .redeem({ exact: amount }, redeemAddress)
      .signAndSend(bashful, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  await observeEvent(
    'funding:RedemptionRequested',
    chainflip,
    (event) => event[0] === bashful.address,
  );
  console.log('Observed RedemptionRequested event');

  const wallet = Wallet.fromMnemonic(
    process.env.ETH_USDC_WHALE_MNEMONIC ??
      'test test test test test test test test test test test junk',
  ).connect(ethers.getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const options: any = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: getEthContractAddress('GATEWAY'),
  };

  // Add 20 seconds extra to guard against any race conditions
  const delay = (await getRedemptionDelay(options)) + 20;
  console.log(`Waiting for ${delay}s before we can execute redemption`);
  await sleep(delay * 1000);

  console.log(`Executing redemption`);
  const accountIdHex = `0x${Buffer.from(bashful.addressRaw).toString('hex')}`;

  const nonce = await getNextEthNonce();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await executeRedemption(accountIdHex as any, { nonce, ...options });

  await observeEvent(
    'funding:RedemptionSettled',
    chainflip,
    (event) => event[0] === bashful.address && event[1] === amount,
  );

  console.log('Observed RedemptionSettled event');

  const newBalance = await observeBalanceIncrease('FLIP', redeemAddress, initBalance);

  console.log(`Redemption success! New balance: ${newBalance.toString()}`);
}
