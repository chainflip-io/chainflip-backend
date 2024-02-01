import { HexString } from '@polkadot/util/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { assetDecimals, fundStateChainAccount } from '@chainflip-io/cli';
import { Wallet, ethers } from 'ethers';
import { getNextEvmNonce } from './send_evm';
import {
  getEvmContractAddress,
  hexPubkeyToFlipAddress,
  decodeFlipAddressForContract,
  getEvmEndpoint,
  getWhaleKey,
} from './utils';
import { observeEvent, getChainflipApi, amountToFineAmount } from '../shared/utils';
import { approveErc20 } from './approve_erc20';

export async function fundFlip(scAddress: string, flipAmount: string) {
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();

  await approveErc20('FLIP', getEvmContractAddress('Ethereum', 'GATEWAY'), flipAmount);

  const flipperinoAmount = amountToFineAmount(flipAmount, assetDecimals.FLIP);

  const flipContractAddress =
    process.env.ETH_FLIP_ADDRESS ?? getEvmContractAddress('Ethereum', 'FLIP');

  const gatewayContractAddress =
    process.env.ETH_GATEWAY_ADDRESS ?? getEvmContractAddress('Ethereum', 'GATEWAY');

  const whaleKey = getWhaleKey('Ethereum');
  console.log('Approving ' + flipAmount + ' FLIP to State Chain Gateway');

  const wallet = new Wallet(whaleKey, ethers.getDefaultProvider(getEvmEndpoint('Ethereum')));

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: gatewayContractAddress,
    flipContractAddress,
  } as const;
  const txOptions = {
    nonce: BigInt(await getNextEvmNonce('Ethereum')),
  } as const;

  console.log('Funding ' + flipAmount + ' FLIP to ' + scAddress);
  let pubkey = scAddress;
  try {
    pubkey = decodeFlipAddressForContract(scAddress);
  } catch {
    // ignore error
  }
  if (pubkey.substr(0, 2) !== '0x') {
    pubkey = '0x' + pubkey;
  }
  const receipt2 = await fundStateChainAccount(
    pubkey as HexString,
    flipperinoAmount,
    networkOptions,
    txOptions,
  );

  console.log(
    'Transaction complete, tx_hash: ' +
      receipt2.transactionHash +
      ' blockNumber: ' +
      receipt2.blockNumber +
      ' blockHash: ' +
      receipt2.blockHash,
  );
  await observeEvent(
    'funding:Funded',
    chainflip,
    (event) => hexPubkeyToFlipAddress(pubkey) === event.data.accountId,
  );
}
