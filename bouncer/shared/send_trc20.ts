import {
  amountToFineAmount,
  getEncodedTronAddress,
  getTronWebClient,
  getTronWhaleKeyPair,
} from 'shared/utils';
import { getErc20abi } from 'shared/contract_interfaces';
import { Logger } from 'shared/utils/logger';

const trc20abi = await getErc20abi();

export async function sendTrc20(
  logger: Logger,
  destinationAddress: string,
  contractAddress: string,
  amount: string,
) {
  const tronWeb = getTronWebClient();
  const privateKey = getTronWhaleKeyPair().privkey;
  tronWeb.setPrivateKey(privateKey);

  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const contract = tronWeb.contract(trc20abi as any, contractAddress);

    const decimals = await contract.decimals().call();
    const symbol = await contract.symbol().call();

    const fineAmount = amountToFineAmount(amount, decimals);

    const encodedAddress = getEncodedTronAddress(destinationAddress);

    logger.debug(`Transferring ${amount} ${symbol} to ${encodedAddress}`);

    const result = await contract
      .transfer(encodedAddress, fineAmount)
      .send({ feeLimit: 100_000_000 }, privateKey);

    logger.info(`Transaction complete, tx_hash: ${result}`);
  } catch (error) {
    logger.error(`sendTrc20 failed: ${error instanceof Error ? error.message : String(error)}`, {
      error,
    });
    throw error;
  }
}
