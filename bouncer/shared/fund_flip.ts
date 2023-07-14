import Web3 from 'web3';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { HexString } from '@polkadot/util/types';
import { fundStateChainAccount } from '@chainflip-io/cli';
import { Wallet, ethers } from 'ethers';
import { getNextEthNonce } from './send_eth';
import { getEthContractAddress, hexPubkeyToFlipAddress } from './utils';
import erc20abi from '../../eth-contract-abis/IERC20.json';
import {
  observeEvent,
  getChainflipApi,
  assetToDecimals,
  amountToFineAmount,
} from '../shared/utils';

export async function fundFlip(pubkey: HexString, flipAmount: string) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();

  const flipperinoAmount = amountToFineAmount(flipAmount, assetToDecimals.get('FLIP')!);

  const web3 = new Web3(ethEndpoint);

  const flipContractAddress = process.env.ETH_FLIP_ADDRESS ?? getEthContractAddress('FLIP');

  const gatewayContractAddress =
    process.env.ETH_GATEWAY_ADDRESS ?? getEthContractAddress('GATEWAY');

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const flipContract = new web3.eth.Contract(erc20abi as any, flipContractAddress);

  const txData = flipContract.methods.approve(gatewayContractAddress, flipperinoAmount).encodeABI();
  const whaleKey =
    process.env.ETH_USDC_WHALE ||
    '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
  console.log('Approving ' + flipAmount + ' FLIP to State Chain Gateway');

  const nonce = await getNextEthNonce();
  const tx = { to: flipContractAddress, data: txData, gas: 2000000, nonce };
  const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
  const receipt = await web3.eth.sendSignedTransaction(
    signedTx.rawTransaction as string,
    (error, hash) => {
      if (error) {
        console.error('Ethereum transaction failure:', error);
      }
    },
  );
  console.log(
    'Transaction complete, tx_hash: ' +
      receipt.transactionHash +
      ' blockNumber: ' +
      receipt.blockNumber +
      ' blockHash: ' +
      receipt.blockHash,
  );

  const wallet = Wallet.fromMnemonic(
    process.env.ETH_USDC_WHALE_MNEMONIC ??
      'test test test test test test test test test test test junk',
  ).connect(ethers.getDefaultProvider(process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'));

  const options: any = {
    signer: wallet,
    network: 'localnet',
    stateChainGatewayContractAddress: getEthContractAddress('GATEWAY'),
    flipContractAddress: getEthContractAddress('FLIP'),
  };

  // TODO: provide nonce manually once it is supported in the SDK/CLI
  const receipt2 = await fundStateChainAccount(pubkey, flipperinoAmount, options);

  console.log('Funding ' + flipAmount + ' FLIP to ' + pubkey);

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
    (data) => hexPubkeyToFlipAddress(pubkey) === data[0],
  );
}
