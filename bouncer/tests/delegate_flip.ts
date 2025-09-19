import Web3 from 'web3';
import { randomBytes } from 'crypto';
import { HDNodeWallet } from 'ethers';
import {
  amountToFineAmountBigInt,
  createEvmWalletAndFund,
  createStateChainKeypair,
  decodeFlipAddressForContract,
  defaultAssetAmounts,
  getContractAddress,
  getEvmEndpoint,
  hexPubkeyToFlipAddress,
  newAddress,
} from 'shared/utils';
import { getIsoTime, Logger } from 'shared/utils/logger';
import { approveErc20 } from 'shared/approve_erc20';
import { newStatechainAddress } from 'shared/new_statechain_address';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { newCcmMetadata } from 'shared/swapping';
import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { setupOperatorAccount } from 'shared/setup_account';
import z from 'zod';

const evmCallDetails = z.object({
  calldata: z.string(),
  value: z.string(),
  to: z.string(),
  source_token_address: z.string().optional(),
});

async function encodeAndSendDelegationApiCall(
  logger: Logger,
  evmWallet: HDNodeWallet,
  call: DelegationApi,
): Promise<string> {
  await using chainflip = await getChainflipApi();

  logger.info(`Requesting EVM encoding for ${evmWallet.address} ${JSON.stringify(call)}`);

  const payload = await chainflip.rpc('cf_evm_calldata', evmWallet.address, {
    API: 'Delegation',
    call,
  });

  logger.info(
    `EVM Call payload for ${evmWallet.address} ${JSON.stringify(call)}: ${JSON.stringify(payload)}`,
  );

  const { calldata, value, to } = evmCallDetails.parse(payload);

  const tx = {
    to,
    data: calldata,
    value,
    gas: 100000,
  };

  const web3 = new Web3(getEvmEndpoint('Ethereum'));
  const signedTx = await web3.eth.accounts.signTransaction(tx, evmWallet.privateKey);
  const receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);

  return receipt.transactionHash;
}

type DelegationApi =
  | { Delegate: { operator: string; increase: { Some: string } | 'Max' } }
  | { Undelegate: { decrease: { Some: string } | 'Max' } }
  | {
      Redeem: {
        amount: { Max: true } | { Exact: string };
        address: string;
        executor?: string;
      };
    };

// Left pad the EVM address to convert it to a Statechain address.
function evmToScAddress(evmAddress: string) {
  return hexPubkeyToFlipAddress('0x' + evmAddress.slice(2).padStart(64, '0'));
}

export async function testDelegate(logger: Logger) {
  // The operator name has to be unique across bouncer runs,
  // since if the test is run the second time for an account
  // that's already registered, it won't emit the `funding:Funded`
  // event
  const uri = `//Operator_0_${getIsoTime()}`;
  logger.debug(`Uri for unique operator account is: "${uri}"`);

  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const wallet = await createEvmWalletAndFund(logger, 'Flip');

  const amount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip');

  const operator = createStateChainKeypair(uri);
  let operatorPubkey = decodeFlipAddressForContract(operator.address);
  if (operatorPubkey.substr(0, 2) !== '0x') {
    operatorPubkey = '0x' + operatorPubkey;
  }

  logger.info('Registering operator ' + operator.address + '...');
  await setupOperatorAccount(logger, uri);

  logger.info('Approving Flip to SC Utils contract for delegation...');
  await approveErc20(logger, 'Flip', scUtilsAddress, amount.toString(), wallet);

  logger.info(`Delegating ${amount} Flip to operator ${operator.address}...`);
  const delegateTxHash = await encodeAndSendDelegationApiCall(logger, wallet, {
    Delegate: { operator: operator.address, increase: { Some: '0x' + amount.toString(16) } },
  });
  logger.info('Delegate flip transaction sent ' + delegateTxHash);

  const fundEvent = observeEvent(logger, 'funding:Funded', {
    test: (event) => {
      const txMatch = event.data.txHash === delegateTxHash;
      const amountMatch = event.data.fundsAdded.replace(/,/g, '') === amount.toString();
      const accountIdMatch = evmToScAddress(wallet.address) === event.data.accountId;
      return txMatch && amountMatch && accountIdMatch;
    },
    historicalCheckBlocks: 10,
  }).event;
  let scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => {
      const txMatch = event.data.ethTxHash === delegateTxHash;
      const operatorMatch =
        event.data.scCall.Delegation.call.Delegate.operator === operator.address;
      return txMatch && operatorMatch;
    },
    historicalCheckBlocks: 10,
  }).event;
  const delegatedEvent = observeEvent(logger, 'validator:Delegated', {
    test: (event) => {
      logger.debug('Delegated event data: ' + JSON.stringify(event.data));
      const delegatorMatch = event.data.delegator === evmToScAddress(wallet.address);
      const operatorMatch = event.data.operator === operator.address;
      return delegatorMatch && operatorMatch;
    },
    historicalCheckBlocks: 10,
  }).event;
  await Promise.all([fundEvent, scCallExecutedEvent, delegatedEvent]);

  logger.info('Undelegating Flip from operator ' + operator.address + '...');
  const undelegateTxHash = await encodeAndSendDelegationApiCall(logger, wallet, {
    Undelegate: { decrease: 'Max' },
  });
  logger.info('Undelegate flip transaction sent ' + undelegateTxHash);

  scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
    test: (event) => event.data.ethTxHash === undelegateTxHash,
  }).event;
  const undelegatedEvent = observeEvent(logger, 'validator:Undelegated', {
    test: (event) => {
      const delegatorMatch = event.data.delegator === evmToScAddress(wallet.address);
      const operatorMatch = event.data.operator === operator.address;
      return delegatorMatch && operatorMatch;
    },
  }).event;
  await Promise.all([scCallExecutedEvent, undelegatedEvent]);

  await using chainflip = await getChainflipApi();
  const pendingRedemption = await chainflip.query.flip.pendingRedemptionsReserve(
    evmToScAddress(wallet.address),
  );

  // Redeem only if there are no other redemptions to prevent queuing issues when
  // running this test multiple times.
  if (pendingRedemption.toString().length === 0) {
    logger.info('Redeeming funds');

    const redeemAddress = await newAddress('Flip', randomBytes(32).toString('hex'));
    const redeemAmount = amount / 2n; // Leave enough to pay fees

    const redeemTxHash = await encodeAndSendDelegationApiCall(logger, wallet, {
      Redeem: { amount: { Exact: '0x' + redeemAmount.toString(16) }, address: redeemAddress },
    });
    logger.info('Redeem request transaction sent ' + redeemTxHash);

    scCallExecutedEvent = observeEvent(logger, 'funding:SCCallExecuted', {
      test: (event) => event.data.ethTxHash === redeemTxHash,
    }).event;
    const redeemEvent = observeEvent(logger, 'funding:RedemptionRequested', {
      test: (event) => {
        const accountMatch = event.data.accountId === evmToScAddress(wallet.address);
        const amountMatch = event.data.amount.replace(/,/g, '') === redeemAmount.toString();
        return accountMatch && amountMatch;
      },
    }).event;
    await Promise.all([scCallExecutedEvent, redeemEvent]);
  }

  logger.info('Delegation test completed successfully!');
}

export async function testCcmSwapFundAccount(logger: Logger) {
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
