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
  newAddress,
} from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { getEthScUtilsAbi } from 'shared/contract_interfaces';
import { approveErc20 } from 'shared/approve_erc20';
import { newStatechainAddress } from 'shared/new_statechain_address';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { newCcmMetadata } from 'shared/swapping';
import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { setupOperatorAccount } from 'shared/setup_account';
import z from 'zod';

const cfScUtilsAbi = await getEthScUtilsAbi();

const evmCallDetails = z.object({
  calldata: z.string(),
  value: z.bigint(),
  to: z.string(),
  source_token_address: z.string().optional(),
});

async function encodeAndSendDelegationApiCall(
  logger: Logger,
  caller: string,
  call: DelegationApi,
): Promise<{ transactionHash: string }> {
  await using chainflip = await getChainflipApi();

  const payload = await chainflip.rpc.call('cf_evm_calldata', caller, { API: 'Delegation', call });

  logger.info(`EVM Call payload for ${caller} ${call}: ${payload}`);

  const { calldata, value, to } = evmCallDetails.parse(payload);

  const { transactionHash } = await signAndSendTxEvm(
    logger,
    'Ethereum',
    to,
    value.toString(),
    calldata,
  );

  return { transactionHash };
}

type DelegationApi =
  | { delegate: { operatorId: string; increase: bigint | 'Max' } }
  | { undelegate: { decrease: bigint | 'Max' } }
  | {
      redeem: {
        amount: { Max: true } | { Exact: bigint };
        address: string;
        executor?: string;
      };
    };

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
  const { pubkey: whalePubkey } = getEvmWhaleKeypair('Ethereum');

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
  const delegateTxHash = await encodeAndSendDelegationApiCall(logger, whalePubkey, {
    delegate: { operatorId: operator.address, increase: amount },
  });
  logger.info('Delegate flip transaction sent ' + delegateTxHash);

  const fundEvent = observeEvent(logger, 'funding:Funded', {
    test: (event) => {
      const txMatch = event.data.txHash === delegateTxHash;
      const amountMatch = event.data.fundsAdded.replace(/,/g, '') === amount.toString();
      const accountIdMatch = evmToScAddress(whalePubkey) === event.data.accountId;
      return txMatch && amountMatch && accountIdMatch;
    },
  }).event;
  let scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => {
      const txMatch = event.data.ethTxHash === delegateTxHash;
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
  const undelegateTxHash = await encodeAndSendDelegationApiCall(logger, whalePubkey, {
    undelegate: { decrease: 'Max' },
  });
  logger.info('Undelegate flip transaction sent ' + undelegateTxHash);

  scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => event.data.ethTxHash === undelegateTxHash,
  }).event;
  const undelegatedEvent = observeEvent(logger, 'validator:Undelegated', {
    test: (event) => {
      const delegatorMatch = event.data.delegator === evmToScAddress(whalePubkey);
      const operatorMatch = event.data.operator === operator.address;
      return delegatorMatch && operatorMatch;
    },
  }).event;
  await Promise.all([scCallExecutedEvent, undelegatedEvent]);

  await using chainflip = await getChainflipApi();
  const pendingRedemption = await chainflip.query.flip.pendingRedemptionsReserve(
    evmToScAddress(whalePubkey),
  );

  // Redeem only if there are no other redemptions to prevent queuing issues when
  // running this test multiple times.
  if (pendingRedemption.toString().length === 0) {
    logger.info('Redeeming funds');

    const redeemAddress = await newAddress('Flip', randomBytes(32).toString('hex'));
    const redemAmount = amount / 2n; // Leave anough to pay fees

    const redeemTxHash = await encodeAndSendDelegationApiCall(logger, whalePubkey, {
      redeem: { amount: { Exact: redemAmount }, address: redeemAddress },
    });
    logger.info('Redeem request transaction sent ' + redeemTxHash);

    scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
      test: (event) => event.data.ethTxHash === redeemTxHash,
    }).event;
    const redeemEvent = observeEvent(logger, 'funding:RedemptionRequested', {
      test: (event) => {
        const accountMatch = event.data.accountId === evmToScAddress(whalePubkey);
        const amountMatch = event.data.amount.replace(/,/g, '') === redemAmount.toString();
        return accountMatch && amountMatch;
      },
    }).event;
    await Promise.all([scCallExecutedEvent, redeemEvent]);
  }

  logger.info('Delegation test completed successfully!');
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
