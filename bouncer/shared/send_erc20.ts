import Web3 from 'web3';
import { signAndSendTxEth } from './send_eth';
import erc20abi from '../../eth-contract-abis/IERC20.json';
import { amountToFineAmount } from './utils';

export async function sendErc20(
  destinationAddress: string,
  contractAddress: string,
  amount: string,
) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = new web3.eth.Contract(erc20abi as any, contractAddress);
  const decimals = await contract.methods.decimals().call();
  const symbol = await contract.methods.symbol().call();

  const fineAmount = amountToFineAmount(amount, decimals);

  const txData = contract.methods.transfer(destinationAddress, fineAmount).encodeABI();

  console.log('Transferring ' + amount + ' ' + symbol + ' to ' + destinationAddress);

  await signAndSendTxEth(contractAddress, txData);
}
