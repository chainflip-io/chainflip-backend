import Web3 from 'web3';
import { Chain } from '@chainflip/cli';
import { signAndSendTxEvm } from 'shared/send_evm';
import { amountToFineAmount, getEvmEndpoint } from 'shared/utils';
import { getErc20abi } from 'shared/contract_interfaces';
import { Logger } from 'shared/utils/logger';

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

  return signAndSendTxEvm(logger, chain, contractAddress, '0', txData, undefined);
}
