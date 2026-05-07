import {
  fineAmountToAmount,
  getEncodedTronAddress,
  getTronWebClient,
  getTronWhaleKeyPair,
} from 'shared/utils';
import { getErc20abi } from 'shared/contract_interfaces';

const trc20abi = await getErc20abi();

export async function getTrc20Balance(address: string, contractAddress: string): Promise<string> {
  const tronWeb = getTronWebClient();
  tronWeb.setPrivateKey(getTronWhaleKeyPair().privkey);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = tronWeb.contract(trc20abi as any, contractAddress);

  const decimals = await contract.decimals().call();
  const encodedAddress = getEncodedTronAddress(address);
  const fineBalance: string = await contract.balanceOf(encodedAddress).call();
  return fineAmountToAmount(fineBalance, decimals);
}
