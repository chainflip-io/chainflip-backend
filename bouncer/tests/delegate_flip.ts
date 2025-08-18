import Web3 from 'web3';
import { randomBytes } from 'crypto';
import { signAndSendTxEvm } from 'shared/send_evm';
import {
  amountToFineAmountBigInt,
  createStateChainKeypair,
  decodeFlipAddressForContract,
  defaultAssetAmounts,
  getContractAddress,
  getEvmEndpoint,
  getEvmWhaleKeypair,
  hexPubkeyToFlipAddress,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { getEthScUtilsAbi } from 'shared/contract_interfaces';
import { approveErc20 } from 'shared/approve_erc20';
import { newStatechainAddress } from 'shared/new_statechain_address';
import { observeEvent } from 'shared/utils/substrate';
import { newCcmMetadata } from 'shared/swapping';
import { Struct, Enum, Option, u128, Bytes as TsBytes } from 'scale-ts';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { setupOperatorAccount } from 'shared/setup_account';

const cfScUtilsAbi = await getEthScUtilsAbi();

// TODO: Update this with the rpc encoding once the logic is implemented in PRO-2439.
export const ScCallsCodec = Enum({
  Delegation: Enum({
    Delegate: Struct({
      operator: TsBytes(32),
    }),
    Undelegate: Struct({}),
    SetMaxBid: Struct({
      maybeMaxBid: Option(u128),
    }),
  }),
});

type ScCallPayload =
  | { type: 'Delegate'; operatorId: string }
  | { type: 'Undelegate' }
  | { type: 'SetMaxBid'; maxBid?: bigint };

function encodeToScCall(payload: ScCallPayload): string {
  switch (payload.type) {
    case 'Delegate':
      return u8aToHex(
        ScCallsCodec.enc({
          tag: 'Delegation',
          value: { tag: 'Delegate', value: { operator: hexToU8a(payload.operatorId) } },
        }),
      );
    case 'Undelegate':
      return u8aToHex(
        ScCallsCodec.enc({
          tag: 'Delegation',
          value: { tag: 'Undelegate', value: {} },
        }),
      );
    case 'SetMaxBid':
      return u8aToHex(
        ScCallsCodec.enc({
          tag: 'Delegation',
          value: { tag: 'SetMaxBid', value: { maybeMaxBid: payload.maxBid } },
        }),
      );
    default:
      throw new Error('Invalid payload type');
  }
}

// Left pad the EVM address to convert it to a Statechain address.
function evmToScAddress(evmAddress: string) {
  return hexPubkeyToFlipAddress('0x' + evmAddress.slice(2).padStart(64, '0'));
}

async function testDelegate(parentLogger: Logger) {
  const web3 = new Web3(getEvmEndpoint('Ethereum'));
  const uri = '//Operator_0';
  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const cfScUtilsContract = new web3.eth.Contract(cfScUtilsAbi, scUtilsAddress);
  const logger = parentLogger.child({ tag: 'DelegateFlip' });

  const amount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip');

  const operator = createStateChainKeypair(uri);
  let operatorPubkey = decodeFlipAddressForContract(operator.address);
  if (operatorPubkey.substr(0, 2) !== '0x') {
    operatorPubkey = '0x' + operatorPubkey;
  }

  logger.info('Registering operator ' + operator.address + '...');
  await setupOperatorAccount(logger, uri);

  logger.info('Approving Flip to SC Utils contract for delegation...');
  await approveErc20(logger, 'Flip', scUtilsAddress, amount.toString());

  logger.info(`Delegating ${amount} Flip to operator ${operator.address}...`);
  let scCall = encodeToScCall({
    type: 'Delegate',
    operatorId: operatorPubkey,
  });
  let txData = cfScUtilsContract.methods.depositToScGateway(amount.toString(), scCall).encodeABI();

  let receipt = await signAndSendTxEvm(logger, 'Ethereum', scUtilsAddress, '0', txData);
  logger.info('Delegate flip transaction sent ' + receipt.transactionHash);

  const { pubkey: whalePubkey } = getEvmWhaleKeypair('Ethereum');
  const fundEvent = observeEvent(logger, 'funding:Funded', {
    test: (event) => {
      const txMatch = event.data.txHash === receipt.transactionHash;
      const amountMatch = event.data.fundsAdded.replace(/,/g, '') === amount.toString();
      const accountIdMatch = evmToScAddress(whalePubkey) === event.data.accountId;
      return txMatch && amountMatch && accountIdMatch;
    },
  }).event;
  let scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => {
      const txMatch = event.data.ethTxHash === receipt.transactionHash;
      const operatorMatch = event.data.scCall.Delegation.Delegate.operator === operator.address;
      return txMatch && operatorMatch;
    },
  }).event;
  const delegatedEvent = observeEvent(logger, 'validator:Delegated', {
    test: (event) => {
      const delegatorMatch = event.data.delegator === evmToScAddress(whalePubkey);
      const operatorMatch = event.data.operator === operator.address;
      return delegatorMatch && operatorMatch;
    },
  }).event;
  await Promise.all([fundEvent, scCallExecutedEvent, delegatedEvent]);

  logger.info('Undelegating Flip from operator ' + operator.address + '...');
  scCall = encodeToScCall({
    type: 'Undelegate',
  });
  txData = cfScUtilsContract.methods.depositToScGateway(amount.toString(), scCall).encodeABI();
  receipt = await signAndSendTxEvm(logger, 'Ethereum', scUtilsAddress, '0', txData);
  logger.info('Undelegate flip transaction sent ' + receipt.transactionHash);

  scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => event.data.ethTxHash === receipt.transactionHash,
  }).event;
  const undelegatedEvent = observeEvent(logger, 'validator:UnDelegated', {
    test: (event) => {
      const delegatorMatch = event.data.delegator === evmToScAddress(whalePubkey);
      const operatorMatch = event.data.operator === operator.address;
      return delegatorMatch && operatorMatch;
    },
  }).event;
  await Promise.all([scCallExecutedEvent, undelegatedEvent]);

  logger.info('Setting new max bid');
  const maxBid = amount;
  scCall = encodeToScCall({
    type: 'SetMaxBid',
    maxBid: BigInt(maxBid),
  });
  txData = cfScUtilsContract.methods.callSc(scCall).encodeABI();
  receipt = await signAndSendTxEvm(logger, 'Ethereum', scUtilsAddress, '0', txData);
  logger.info('Set Max Bid transaction sent ' + receipt.transactionHash);

  scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => event.data.ethTxHash === receipt.transactionHash,
  }).event;
  const maxBidEvent = observeEvent(logger, 'validator:MaxBidUpdated', {
    test: (event) => {
      const delegatorMatch = event.data.delegator === evmToScAddress(whalePubkey);
      const maxBidMatch = event.data.maxBid.replace(/,/g, '') === maxBid.toString();
      return delegatorMatch && maxBidMatch;
    },
  }).event;
  await Promise.all([scCallExecutedEvent, maxBidEvent]);

  logger.info('Delegation and undelegation test completed successfully!');
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
