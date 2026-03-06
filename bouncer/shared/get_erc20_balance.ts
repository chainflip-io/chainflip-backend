import { ChainflipChain as Chain } from '@chainflip/utils/chainflip';
import { fineAmountToAmount, getWeb3 } from 'shared/utils';
import { getErc20abi } from 'shared/contract_interfaces';

const erc20abi = await getErc20abi();

export async function getErc20Balance(
  chain: Chain,
  walletAddress: string,
  contractAddress: string,
): Promise<string> {
  const web3 = getWeb3(chain);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = new web3.eth.Contract(erc20abi as any, contractAddress);

  const decimals = await contract.methods.decimals().call();
  const fineBalance: string = await contract.methods.balanceOf(walletAddress).call();
  return fineAmountToAmount(fineBalance, decimals);
}
