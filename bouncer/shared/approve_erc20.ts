import Web3 from 'web3';
import { Asset } from '@chainflip-io/cli';
import { amountToFineAmount } from '../shared/utils';
import { getEthContractAddress } from './utils';
import { signAndSendTxEth } from './send_eth';
import { getErc20abi } from './eth_abis';

const erc20abi = await getErc20abi();

export async function approveErc20(asset: Asset, toAddress: string, amount: string) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';

  const web3 = new Web3(ethEndpoint);

  const tokenContractAddress = getEthContractAddress(asset);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const tokenContract = new web3.eth.Contract(erc20abi as any, tokenContractAddress);
  const decimals = await tokenContract.methods.decimals().call();
  const tokenAmount = amountToFineAmount(amount, decimals);

  const txData = tokenContract.methods.approve(toAddress, tokenAmount).encodeABI();

  console.log('Approving ' + amount + ' ' + asset + ' to ' + toAddress);

  await signAndSendTxEth(tokenContractAddress, '0', txData);
}
