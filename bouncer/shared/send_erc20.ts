import Web3 from 'web3';
import { signAndSendTxEth } from './send_eth';
import { amountToFineAmount } from './utils';
import { getErc20abi } from './eth_abis';

const erc20abi = await getErc20abi();

export async function sendErc20(
  destinationAddress: string,
  contractAddress: string,
  amount: string,
  log = true,
) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = new web3.eth.Contract(erc20abi as any, contractAddress);
  const decimals = await contract.methods.decimals().call();
  const symbol = await contract.methods.symbol().call();

  const fineAmount = amountToFineAmount(amount, decimals);

  const txData = contract.methods.transfer(destinationAddress, fineAmount).encodeABI();

  if (log) console.log('Transferring ' + amount + ' ' + symbol + ' to ' + destinationAddress);

  await signAndSendTxEth(contractAddress, '0', txData, undefined, log);
}
