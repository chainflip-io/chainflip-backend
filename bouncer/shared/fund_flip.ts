import { HexString } from '@polkadot/util/types';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { assetDecimals, fundStateChainAccount } from '@chainflip-io/cli';
import { Wallet, ethers } from 'ethers';
import { getNextEthNonce } from './send_eth';
import {
  getEthContractAddress,
  hexPubkeyToFlipAddress,
  decodeFlipAddressForContract,
} from './utils';
import { observeEvent, getChainflipApi, amountToFineAmount } from '../shared/utils';
import { approveErc20 } from './approve_erc20';

export async function fundFlip(address: string, flipAmount: string) {
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();

  await approveErc20('FLIP', getEthContractAddress('GATEWAY'), flipAmount);

  const flipperinoAmount = amountToFineAmount(flipAmount, assetDecimals.FLIP);

  const flipContractAddress = process.env.ETH_FLIP_ADDRESS ?? getEthContractAddress('FLIP');

  const gatewayContractAddress =
    process.env.ETH_GATEWAY_ADDRESS ?? getEthContractAddress('GATEWAY');

  const whaleKey =
    process.env.ETH_USDC_WHALE ||
    '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
  console.log('Approving ' + flipAmount + ' FLIP to State Chain Gateway');

  const wallet = new Wallet(
    whaleKey,
    ethers.getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'),
  );

  const networkOptions = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: gatewayContractAddress,
    flipContractAddress,
  } as const;
  const txOptions = {
    nonce: BigInt(await getNextEthNonce()),
  } as const;

  console.log('Funding ' + flipAmount + ' FLIP to ' + address);
  let pubkey = address;
  try {
    pubkey = decodeFlipAddressForContract(address);
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
