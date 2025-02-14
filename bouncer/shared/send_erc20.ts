import Web3 from 'web3';
import { Chain } from '@chainflip/cli';
import { signAndSendTxEvm } from './send_evm';
import { amountToFineAmount, getEvmEndpoint } from './utils';
import { getErc20abi } from './contract_interfaces';
import { Logger } from './utils/logger';

const erc20abi = await getErc20abi();

export async function sendErc20(
  logger: Logger,
  chain: Chain,
  destinationAddress: string,
  contractAddress: string,
  amount: string,
) {
  const web3 = new Web3(getEvmEndpoint(chain));

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const contract = new web3.eth.Contract(erc20abi as any, contractAddress);
  const decimals = await contract.methods.decimals().call();
  const symbol = await contract.methods.symbol().call();

  const fineAmount = amountToFineAmount(amount, decimals);

  const txData = contract.methods.transfer(destinationAddress, fineAmount).encodeABI();

  logger.trace(`Transferring ${amount} ${symbol} to ${destinationAddress}`);

  await signAndSendTxEvm(logger, chain, contractAddress, '0', txData, undefined);
}
