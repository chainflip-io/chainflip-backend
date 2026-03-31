import { HDNodeWallet } from 'ethers';
import {
  amountToFineAmount,
  chainFromAsset,
  getWeb3,
  getContractAddress,
  Asset,
} from 'shared/utils';
import { signAndSendTxEvm } from 'shared/send_evm';
import { getErc20abi } from 'shared/contract_interfaces';
import { Logger } from 'shared/utils/logger';

const erc20abi = await getErc20abi();

export async function approveErc20(
  logger: Logger,
  asset: Asset,
  toAddress: string,
  amount: string,
  evmWallet?: HDNodeWallet,
) {
  const chain = chainFromAsset(asset);

  const web3 = getWeb3(chain);

  const tokenContractAddress = getContractAddress(chain, asset);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const tokenContract = new web3.eth.Contract(erc20abi as any, tokenContractAddress);
  const decimals = await tokenContract.methods.decimals().call();
  const tokenAmount = amountToFineAmount(amount, decimals);

  const txData = tokenContract.methods.approve(toAddress, tokenAmount).encodeABI();

  logger.debug('Approving ' + amount + ' ' + asset + ' to ' + toAddress);

  // Uses the default whale keypair if no wallet is provided
  await signAndSendTxEvm(
    logger,
    chain,
    { to: tokenContractAddress, value: '0', data: txData },
    { privateKey: evmWallet?.privateKey },
  );
}
