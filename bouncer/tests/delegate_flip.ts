import Web3 from 'web3';
import { randomBytes } from 'crypto';
import { signAndSendTxEvm } from 'shared/send_evm';
import {
  amountToFineAmountBigInt,
  decodeFlipAddressForContract,
  defaultAssetAmounts,
  getContractAddress,
  getEvmEndpoint,
  hexPubkeyToFlipAddress,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { getEthScUtilsAbi } from 'shared/contract_interfaces';
import { approveErc20 } from 'shared/approve_erc20';
import { newStatechainAddress } from 'shared/new_statechain_address';
import { observeEvent } from 'shared/utils/substrate';
import { newCcmMetadata } from 'shared/swapping';
import { Struct, Enum, Bytes as TsBytes } from 'scale-ts';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';

const cfScUtilsAbi = await getEthScUtilsAbi();

export const ScCallsCodec = Enum({
  DelegateTo: Struct({
    operator: TsBytes(32),
  }),
  // TODO: add others
});

function encodeDelegateToScCall(operatorId: string) {
  return u8aToHex(
    ScCallsCodec.enc({
      tag: 'DelegateTo',
      value: { operator: hexToU8a(operatorId) },
    }),
  );
}

async function testDelegate(parentLogger: Logger) {
  const web3 = new Web3(getEvmEndpoint('Ethereum'));
  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const cfScUtilsContract = new web3.eth.Contract(cfScUtilsAbi, scUtilsAddress);
  const logger = parentLogger.child({ tag: 'DelegateFlip' });

  const amount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip');

  logger.info('Approving Flip to SC Utils contract for deposit...');
  await approveErc20(logger, 'Flip', scUtilsAddress, amount.toString());
  logger.debug('Approved FLIP');

  // Encoding dummy
  const scCall = encodeDelegateToScCall(
    '0xf4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4f4',
  );
  const txData = cfScUtilsContract.methods
    .depositToScGateway(amount.toString(), scCall)
    .encodeABI();

  const receipt = await signAndSendTxEvm(logger, 'Ethereum', scUtilsAddress, '0', txData);
  logger.info('Delegate flip transaction sent ' + receipt.transactionHash);

  // TODO: Check the correct behavior in the SC once the logic is implemented.
}

async function testCcmSwapFundAccount(logger: Logger) {
  const web3 = new Web3(getEvmEndpoint('Ethereum'));
  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const scAddress = await newStatechainAddress(randomBytes(32).toString('hex'));

  let pubkey = decodeFlipAddressForContract(scAddress);
  if (pubkey.substr(0, 2) !== '0x') {
    pubkey = '0x' + pubkey;
  }
  const fundEvent = observeEvent(logger, 'funding:Funded', {
    test: (event) => hexPubkeyToFlipAddress(pubkey) === event.data.accountId,
  }).event;

  const ccmMessage = web3.eth.abi.encodeParameters(['address', 'bytes'], [scUtilsAddress, pubkey]);
  const ccmMetadata = await newCcmMetadata('Flip', ccmMessage);
  // Override gas budget for this particular use case
  ccmMetadata.gasBudget = '1000000';

  const swapParams = await requestNewSwap(logger, 'Btc', 'Flip', scUtilsAddress, ccmMetadata);

  await send(logger, 'Btc', swapParams.depositAddress);
  await fundEvent;
  logger.info('Funding event witnessed succesfully!');
}

export async function testDelegateFlip(testContext: TestContext) {
  await Promise.all([testDelegate(testContext.logger), testCcmSwapFundAccount(testContext.logger)]);
}
