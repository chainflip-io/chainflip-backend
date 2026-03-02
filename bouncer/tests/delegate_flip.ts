import { randomBytes } from 'crypto';
import { HDNodeWallet } from 'ethers';
import {
  amountToFineAmountBigInt,
  createEvmWalletAndFund,
  decodeFlipAddressForContract,
  defaultAssetAmounts,
  externalChainToScAccount,
  getContractAddress,
  getWeb3,
  hexPubkeyToFlipAddress,
  newAddress,
  observeSwapRequested,
  SwapRequestType,
  TransactionOrigin,
} from 'shared/utils';
import { getIsoTime, Logger } from 'shared/utils/logger';
import { approveErc20 } from 'shared/approve_erc20';
import { newStatechainAddress } from 'shared/new_statechain_address';
import { getChainflipApi } from 'shared/utils/substrate';
import { newCcmMetadata } from 'shared/swapping';
import { requestNewSwap } from 'shared/perform_swap';
import { send } from 'shared/send';
import { AccountRole, setupAccount } from 'shared/setup_account';
import z from 'zod';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { TestContext } from 'shared/utils/test_context';
import { fundingFunded } from 'generated/events/funding/funded';
import { fundingSCCallExecuted } from '../generated/events/funding/sCCallExecuted';
import { validatorDelegated } from '../generated/events/validator/delegated';
import { validatorUndelegated } from '../generated/events/validator/undelegated';
import { fundingRedemptionRequested } from '../generated/events/funding/redemptionRequested';

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

  const web3 = getWeb3('Ethereum');
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

export async function testDelegate(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);

  // The operator name has to be unique across bouncer runs,
  // since if the test is run the second time for an account
  // that's already registered, it won't emit the `funding:Funded`
  // event
  const uri: `//${string}` = `//Operator_0_${getIsoTime()}`;
  cf.debug(`Uri for unique operator account is: "${uri}"`);

  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const wallet = await createEvmWalletAndFund(cf.logger, 'Flip');
  const amountString = defaultAssetAmounts('Flip');

  const amount = amountToFineAmountBigInt(amountString, 'Flip');

  cf.info('Registering operator ' + uri + '...');
  const operator = await setupAccount(cf, uri, AccountRole.Operator);

  let operatorPubkey = decodeFlipAddressForContract(operator.address);
  if (operatorPubkey.substr(0, 2) !== '0x') {
    operatorPubkey = '0x' + operatorPubkey;
  }

  cf.info('Approving Flip to SC Utils contract for delegation...');
  await approveErc20(cf.logger, 'Flip', scUtilsAddress, amount.toString(), wallet);

  cf.info(`Delegating ${amount} Flip to operator ${operator.address}...`);
  const delegateTxHash = await encodeAndSendDelegationApiCall(cf.logger, wallet, {
    Delegate: { operator: operator.address, increase: { Some: '0x' + amount.toString(16) } },
  });
  cf.info('Delegate flip transaction sent ' + delegateTxHash);

  await cf.stepUntilAllEventsOf({
    funding: {
      name: 'Funding.Funded',
      schema: fundingFunded.refine(
        (event) =>
          event.accountId === externalChainToScAccount(wallet.address) &&
          event.source.__kind === 'EthTransaction' &&
          event.source.txHash === delegateTxHash &&
          event.fundsAdded === amount,
      ),
    },
    scCallExecuted: {
      name: 'Funding.SCCallExecuted',
      schema: fundingSCCallExecuted.refine(
        (event) =>
          event.ethTxHash === delegateTxHash &&
          event.scCall.call.__kind === 'Delegate' &&
          event.scCall.call.operator === operator.address,
      ),
    },
    validatorDelegated: {
      name: 'Validator.Delegated',
      schema: validatorDelegated.refine(
        (event) =>
          event.delegator === externalChainToScAccount(wallet.address) &&
          event.operator === operator.address,
      ),
    },
  });

  cf.info('Undelegating Flip from operator ' + operator.address + '...');
  const undelegateTxHash = await encodeAndSendDelegationApiCall(cf.logger, wallet, {
    Undelegate: { decrease: 'Max' },
  });
  cf.info('Undelegate flip transaction sent ' + undelegateTxHash);

  await cf.stepUntilAllEventsOf({
    scCallExecuted: {
      name: 'Funding.SCCallExecuted',
      schema: fundingSCCallExecuted.refine(
        (event) =>
          event.ethTxHash === undelegateTxHash && event.scCall.call.__kind === 'Undelegate',
      ),
    },
    validatorDelegated: {
      name: 'Validator.Undelegated',
      schema: validatorUndelegated.refine(
        (event) =>
          event.delegator === externalChainToScAccount(wallet.address) &&
          event.operator === operator.address,
      ),
    },
  });

  await using chainflip = await getChainflipApi();
  const pendingRedemption = await chainflip.query.flip.pendingRedemptionsReserve(
    externalChainToScAccount(wallet.address),
  );

  // Redeem only if there are no other redemptions to prevent queuing issues when
  // running this test multiple times.
  if (pendingRedemption.toString().length === 0) {
    cf.info('Redeeming funds');

    const redeemAddress = await newAddress('Flip', randomBytes(32).toString('hex'));
    const redeemAmount = amount / 2n; // Leave enough to pay fees

    const redeemTxHash = await encodeAndSendDelegationApiCall(cf.logger, wallet, {
      Redeem: { amount: { Exact: '0x' + redeemAmount.toString(16) }, address: redeemAddress },
    });
    cf.info('Redeem request transaction sent ' + redeemTxHash);

    await cf.stepUntilAllEventsOf({
      scCallExecuted: {
        name: 'Funding.SCCallExecuted',
        schema: fundingSCCallExecuted.refine(
          (event) => event.ethTxHash === redeemTxHash && event.scCall.call.__kind === 'Redeem',
        ),
      },
      validatorDelegated: {
        name: 'Funding.RedemptionRequested',
        schema: fundingRedemptionRequested.refine(
          (event) =>
            event.accountId === externalChainToScAccount(wallet.address) &&
            event.amount === redeemAmount,
        ),
      },
    });
  }

  cf.info('Delegation test completed successfully!');
}

export async function testCcmSwapFundAccount(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  const web3 = getWeb3('Ethereum');
  const scUtilsAddress = getContractAddress('Ethereum', 'SC_UTILS');
  const scAddress = await newStatechainAddress(randomBytes(32).toString('hex'));

  let pubkey = decodeFlipAddressForContract(scAddress);
  if (pubkey.substr(0, 2) !== '0x') {
    pubkey = '0x' + pubkey;
  }

  const ccmMessage = web3.eth.abi.encodeParameters(['address', 'bytes'], [scUtilsAddress, pubkey]);
  const ccmMetadata = await newCcmMetadata('Flip', ccmMessage);
  // Override gas budget for this particular use case
  ccmMetadata.gasBudget = '1000000';

  const swapParams = await requestNewSwap(cf, 'Btc', 'Flip', scUtilsAddress, ccmMetadata);

  await send(cf.logger, 'Btc', swapParams.depositAddress);

  await observeSwapRequested(
    cf,
    'Btc',
    'Flip',
    { type: TransactionOrigin.DepositChannel, channelId: swapParams.channelId },
    SwapRequestType.Regular,
  );

  await cf.stepUntilEvent(
    'Funding.Funded',
    fundingFunded.refine(
      (event) =>
        event.accountId === hexPubkeyToFlipAddress(pubkey) &&
        event.source.__kind === 'EthTransaction',
    ),
  );

  cf.info('Funding event witnessed succesfully!');
}
