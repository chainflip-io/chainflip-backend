import type { AccountId32 } from 'dedot/codecs';
import { amountToFineAmountBigInt, defaultAssetAmounts } from 'shared/utils';
import { getIsoTime } from 'shared/utils/logger';
import { fundFlip } from 'shared/fund_flip';
import { AccountRole, setupAccount } from 'shared/setup_account';
import { newChainflipIO, partialAccountFromUri } from 'shared/utils/chainflip_io';
import { TestContext } from 'shared/utils/test_context';
import { validatorDelegationPlanUpdatedEvent } from 'generated/events/validator/delegationPlanUpdated';

// The generated `PalletCfValidatorDelegationDelegatorRelations` type is strict about
// `AccountId32` (unlike top-level call params, which accept the more permissive
// `AccountId32Like`), so SS58 addresses from a `KeyringPair` need this cast.
const accountId = (address: string): AccountId32 => address as unknown as AccountId32;

export async function testMultiDelegate(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);

  // Account names have to be unique across bouncer runs, since if the test is run a second
  // time for accounts that are already registered/funded, expected events won't be re-emitted.
  const timestamp = getIsoTime();
  const operatorAUri: `//${string}` = `//Operator_MultiA_${timestamp}`;
  const operatorBUri: `//${string}` = `//Operator_MultiB_${timestamp}`;
  const delegatorUri: `//${string}` = `//Delegator_Multi_${timestamp}`;

  cf.info(`Registering operators ${operatorAUri} and ${operatorBUri}...`);
  const operatorA = await setupAccount(cf, operatorAUri, AccountRole.Operator);
  const operatorB = await setupAccount(cf, operatorBUri, AccountRole.Operator);

  const delegatorCf = cf.with({ account: partialAccountFromUri(delegatorUri) });
  const delegator = delegatorCf.requirements.account.keypair;

  const totalAmount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip');
  const amountToOperatorA = totalAmount / 2n;
  const amountToOperatorB = totalAmount - amountToOperatorA;

  cf.info(`Funding delegator ${delegator.address} with Flip...`);
  await fundFlip(cf, delegator.address, defaultAssetAmounts('Flip'));

  cf.info(
    `Delegating ${amountToOperatorA} to ${operatorA.address} and ${amountToOperatorB} to ${operatorB.address}...`,
  );
  const plan = await delegatorCf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.validator.delegateMulti({
        operators: [
          [accountId(operatorA.address), amountToOperatorA],
          [accountId(operatorB.address), amountToOperatorB],
        ],
      }),
    expectedEvent: validatorDelegationPlanUpdatedEvent.refine(
      (event) => event.delegator === delegator.address,
    ),
  });

  if (
    plan.plan.operators.length !== 2 ||
    !plan.plan.operators.some(
      ([operator, amount]) => operator === operatorA.address && amount === amountToOperatorA,
    ) ||
    !plan.plan.operators.some(
      ([operator, amount]) => operator === operatorB.address && amount === amountToOperatorB,
    )
  ) {
    throw new Error(`Unexpected delegation plan after delegate_multi: ${JSON.stringify(plan)}`);
  }

  cf.info(`Updating delegation plan to only delegate to ${operatorA.address}...`);
  const updatedPlan = await delegatorCf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.validator.delegateMulti({ operators: [[accountId(operatorA.address), totalAmount]] }),
    expectedEvent: validatorDelegationPlanUpdatedEvent.refine(
      (event) => event.delegator === delegator.address,
    ),
  });

  if (
    updatedPlan.plan.operators.length !== 1 ||
    !updatedPlan.plan.operators.some(
      ([operator, amount]) => operator === operatorA.address && amount === totalAmount,
    )
  ) {
    throw new Error(
      `Unexpected delegation plan after switching to a single operator: ${JSON.stringify(updatedPlan)}`,
    );
  }

  cf.info('Undelegating from all operators via an empty plan...');
  const emptyPlan = await delegatorCf.submitExtrinsic({
    extrinsic: (api) => api.tx.validator.delegateMulti({ operators: [] }),
    expectedEvent: validatorDelegationPlanUpdatedEvent.refine(
      (event) => event.delegator === delegator.address,
    ),
  });

  if (emptyPlan.plan.operators.length !== 0) {
    throw new Error(`Expected an empty delegation plan, got: ${JSON.stringify(emptyPlan)}`);
  }

  cf.info('Multi-operator delegation test completed successfully!');
}
