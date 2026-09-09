import { fundFlip } from 'shared/fund_flip';
import { AccountRole, setupAccount } from 'shared/setup_account';
import { newChainflipIO, partialAccountFromUri } from 'shared/utils/chainflip_io';
import { getIsoTime } from 'shared/utils/logger';
import { amountToFineAmountBigInt, defaultAssetAmounts } from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { validatorDelegationPlanUpdatedEvent } from 'generated/events/validator/delegationPlanUpdated';
import type { PalletCfValidatorDelegationDelegatorRelations } from 'generated/chaintypes/chainflip-node/types';

/**
 * `BTreeMap<K, V>` has no distinct scale-info representation -- it derives `TypeInfo` as a
 * `Sequence` of `(K, V)`, identical on the wire to `Vec<(K, V)>`. So dedot's registry builds the
 * ordinary array/tuple codec for it, and a plain array of `[key, value]` pairs (exactly what the
 * generated `Array<[AccountId32, bigint]>` type says) is the correct, and only, encodable shape --
 * there's no dedicated `BTreeMap`/`Map` codec to route through.
 *
 * Passing a native `Map` instead (the intuitive JS analogue of a `BTreeMap`) doesn't hit a "BTreeMap
 * bug" so much as a gap in that array codec: its `subAssert` correctly rejects a `Map` (`instanceof
 * Array` fails, so `delegateMulti(...).sign()` throws a clear `ShapeAssertError`), but its `subEncode`
 * -- used by the lower-level `tryEncode` that some codepaths call directly, bypassing that assert --
 * reads `value.length`, which is `undefined` on a `Map` (only `.size` is defined). `undefined` is
 * falsy, so the length-guarded entry loop is skipped entirely and a *valid, empty* operators list is
 * encoded instead of throwing -- silently dropping every entry rather than failing loudly. That's the
 * real dedot bug: a `Map` should be rejected at encode time, not silently coerced to `[]`.
 *
 * The cast below is only for the `AccountId32` vs `AccountId32Like` mismatch: dedot's codegen reuses
 * the strict decode-side `AccountId32` class as this field's encode-side type too, even though the
 * codec (like every other `AccountId32`-keyed field in these chaintypes) happily accepts a plain SS58
 * address string at runtime.
 */
function toDelegatorOperators(
  entries: [string, bigint][],
): PalletCfValidatorDelegationDelegatorRelations['operators'] {
  return entries as unknown as PalletCfValidatorDelegationDelegatorRelations['operators'];
}

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

  cf.info(`Funding delegator ${delegator.address} with Flip...`);
  await fundFlip(delegatorCf, delegator.address, defaultAssetAmounts('Flip'));

  const totalAmount = amountToFineAmountBigInt(defaultAssetAmounts('Flip'), 'Flip') / 2n;
  const amountToOperatorA = totalAmount / 2n;
  const amountToOperatorB = totalAmount - amountToOperatorA;

  cf.info(
    `Delegating ${amountToOperatorA} to ${operatorA.address} and ${amountToOperatorB} to ${operatorB.address}...`,
  );
  const planUpdated = await delegatorCf.submitExtrinsic({
    extrinsic: (client) =>
      client.tx.validator.delegateMulti({
        operators: toDelegatorOperators([
          [operatorA.address, amountToOperatorA],
          [operatorB.address, amountToOperatorB],
        ]),
      }),
    expectedEvent: validatorDelegationPlanUpdatedEvent,
  });

  const operators = planUpdated.plan.operators;
  if (
    operators.length !== 2 ||
    !operators.some(
      ([operator, amount]) => operator === operatorA.address && amount === amountToOperatorA,
    ) ||
    !operators.some(
      ([operator, amount]) => operator === operatorB.address && amount === amountToOperatorB,
    )
  ) {
    throw new Error(
      `Unexpected delegation plan after delegate_multi: ${JSON.stringify(operators, (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`,
    );
  }

  cf.info(`Updating delegation plan to only delegate to ${operatorA.address}...`);
  const updatedPlanUpdated = await delegatorCf.submitExtrinsic({
    extrinsic: (client) =>
      client.tx.validator.delegateMulti({
        operators: toDelegatorOperators([[operatorA.address, totalAmount]]),
      }),
    expectedEvent: validatorDelegationPlanUpdatedEvent,
  });

  const updatedOperators = updatedPlanUpdated.plan.operators;
  if (
    updatedOperators.length !== 1 ||
    !updatedOperators.some(
      ([operator, amount]) => operator === operatorA.address && amount === totalAmount,
    )
  ) {
    throw new Error(
      `Unexpected delegation plan after switching to a single operator: ${JSON.stringify(updatedOperators, (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`,
    );
  }

  cf.info('Undelegating from all operators via an empty plan...');
  const emptyPlanUpdated = await delegatorCf.submitExtrinsic({
    extrinsic: (client) =>
      client.tx.validator.delegateMulti({ operators: toDelegatorOperators([]) }),
    expectedEvent: validatorDelegationPlanUpdatedEvent,
  });

  if (emptyPlanUpdated.plan.operators.length !== 0) {
    throw new Error(
      `Expected an empty delegation plan, got: ${JSON.stringify(emptyPlanUpdated.plan.operators, (_, v) => (typeof v === 'bigint' ? v.toString() : v))}`,
    );
  }

  cf.info('Multi-operator delegation test completed successfully!');
}
