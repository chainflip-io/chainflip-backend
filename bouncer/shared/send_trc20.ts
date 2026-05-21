import {
  amountToFineAmount,
  getEncodedTronAddress,
  getTronWebClient,
  getTronWhaleKeyPair,
} from 'shared/utils';
import { getErc20abi } from 'shared/contract_interfaces';
import { Logger } from 'shared/utils/logger';
import { HexString } from '@polkadot/util/types';

const trc20abi = await getErc20abi();

export async function sendTrc20(
  logger: Logger,
  destinationAddress: string,
  contractAddress: string,
  amount: string,
): Promise<HexString> {
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
    const txHash: HexString = `0x${result as string}`;

    logger.info(`Transaction complete, tx_hash: ${txHash}`);
    return txHash;
  } catch (error) {
    logger.error(`sendTrc20 failed: ${error instanceof Error ? error.message : String(error)}`, {
      error,
    });
    throw error;
  }
}
