import Web3 from 'web3';
import { signAndSendTxEvm } from 'shared/send_evm';
import {
  amountToFineAmountBigInt,
  defaultAssetAmounts,
  getContractAddress,
  getEvmEndpoint,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { getEthScUtilsAbi } from 'shared/contract_interfaces';
import { approveErc20 } from 'shared/approve_erc20';

const cfScUtilsAbi = await getEthScUtilsAbi();

async function testDelegate(parentLogger: Logger) {
  const web3 = new Web3(getEvmEndpoint('Ethereum'));
  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const cfScUtilsContract = new web3.eth.Contract(cfScUtilsAbi, scUtilsAddress);
  const logger = parentLogger.child({ tag: 'DelegateFlip' });

  const amount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip');

  logger.info('Approving Flip to SC Utils contract for deposit...');
  await approveErc20(logger, 'Flip', scUtilsAddress, amount.toString());
  console.log('Approved FLIP');

  // Encoding dummy
  const scCall = '0x00f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4';
  const txData = cfScUtilsContract.methods
    .depositToScGateway(amount.toString(), scCall)
    .encodeABI();

  const receipt = await signAndSendTxEvm(logger, 'Ethereum', scUtilsAddress, '0', txData);
  logger.info('Delegate flip transaction sent ' + receipt.transactionHash);
}

export async function testDelegateFlip(testContext: TestContext) {
  await Promise.all([testDelegate(testContext.logger)]);
}
