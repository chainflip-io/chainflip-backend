import type { HexString } from '@polkadot/util/types';
import { fundStateChainAccount } from '@chainflip/cli';
import { Wallet, ethers } from 'ethers';
import { Keyring } from './polkadot/keyring';
import { getNextEvmNonce } from './send_evm';
import {
  getContractAddress,
  hexPubkeyToFlipAddress,
  decodeFlipAddressForContract,
  getEvmEndpoint,
  getWhaleKey,
  assetDecimals,
  lpMutex,
  handleSubstrateError,
  amountToFineAmount,
} from './utils';
import { approveErc20 } from './approve_erc20';
import { getChainflipApi, observeEvent } from './utils/substrate';

export async function setupLpAccount(lpKey: string) {
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);
  const lpUri = lpKey;
  const lp = keyring.createFromUri(lpUri);

  await approveErc20('Flip', getContractAddress('Ethereum', 'GATEWAY'), '1000');

  const flipperinoAmount = amountToFineAmount('1000', assetDecimals('Flip'));

  const flipContractAddress =
    process.env.ETH_FLIP_ADDRESS ?? getContractAddress('Ethereum', 'Flip');

  const gatewayContractAddress =
    process.env.ETH_GATEWAY_ADDRESS ?? getContractAddress('Ethereum', 'GATEWAY');

  const whaleKey = getWhaleKey('Ethereum');
  console.log('Approving 1000 Flip to State Chain Gateway');

  const wallet = new Wallet(whaleKey, ethers.getDefaultProvider(getEvmEndpoint('Ethereum')));

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: gatewayContractAddress,
    flipContractAddress,
  } as const;
  const txOptions = {
    nonce: await getNextEvmNonce('Ethereum'),
  } as const;

  console.log('Funding 1000 Flip to ' + lp.address);
  let pubkey = lp.address;
  try {
    pubkey = decodeFlipAddressForContract(lp.address);
  } catch {
    // ignore error
  }
  if (pubkey.substr(0, 2) !== '0x') {
    pubkey = '0x' + pubkey;
  }
  const receipt2 = await fundStateChainAccount(
    pubkey as HexString,
    BigInt(flipperinoAmount),
    networkOptions,
    txOptions,
  );

  console.log(
    'Transaction complete, tx_hash: ' +
      receipt2.hash +
      ' blockNumber: ' +
      receipt2.blockNumber +
      ' blockHash: ' +
      receipt2.blockHash,
  );
  await observeEvent('funding:Funded', {
    test: (event) => hexPubkeyToFlipAddress(pubkey) === event.data.accountId,
  }).event;

  console.log(`Registering ${lp.address} as an LP...`);

  await using chainflip = await getChainflipApi();

  const eventHandle = observeEvent('accountRoles:AccountRoleRegistered', {
    test: (event) => event.data.accountId === lp.address,
  }).event;

  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .registerLpAccount()
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await eventHandle;

  console.log(`${lp.address} successfully registered as an LP`);
}
