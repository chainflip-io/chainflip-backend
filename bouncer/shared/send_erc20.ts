import { Asset, assetDecimals } from '@chainflip-io/cli';
import Web3 from 'web3';
import { getNextEthNonce } from './send_eth';
import erc20abi from '../../eth-contract-abis/IERC20.json';
import { amountToFineAmount, getEthContractAddress } from './utils';

export async function sendErc20(destinationAddress: string, token: Asset, amount: string) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);
  const contractAddress = getEthContractAddress(token);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = new web3.eth.Contract(erc20abi as any, contractAddress);
  const symbol = await contract.methods.symbol().call();

  const fineAmount = amountToFineAmount(amount, assetDecimals[token]);

  const txData = contract.methods.transfer(destinationAddress, fineAmount).encodeABI();
  const whaleKey =
    process.env.ETH_USDC_WHALE ||
    '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
  console.log('Transferring ' + amount + ' ' + symbol + ' to ' + destinationAddress);

  const nonce = await getNextEthNonce();
  const tx = { to: contractAddress, data: txData, gas: 2000000, nonce };

  const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
  const receipt = await web3.eth.sendSignedTransaction(
    signedTx.rawTransaction as string,
    (error) => {
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
}
