import type { SubmittableResult } from '@polkadot/api';
// eslint-disable-next-line no-restricted-imports
import type { KeyringPair } from '@polkadot/keyring/types';
import { fundFlip } from 'shared/fund_flip';
import { AccountRole, setupAccount } from 'shared/setup_account';
import { newChainflipIO, partialAccountFromUri } from 'shared/utils/chainflip_io';
import { getChainflipPolkadotApi } from 'shared/utils/substrate';
import { getIsoTime } from 'shared/utils/logger';
import { amountToFineAmountBigInt, defaultAssetAmounts } from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';

async function submitDelegateMulti(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  polkadotApi: any,
  delegator: KeyringPair,
  operators: Map<string, bigint>,
) {
  return new Promise<SubmittableResult['events']>((resolve, reject) => {
    polkadotApi.tx.validator
      .delegateMulti({ operators })
      .signAndSend(delegator, (result: SubmittableResult) => {
        if (result.dispatchError) {
          reject(new Error(`delegateMulti: dispatch error ${result.dispatchError.toString()}`));
        } else if (result.status.isInBlock || result.status.isFinalized) {
          resolve(result.events);
        }
      })
      .catch(reject);
  });
}

function findEventData(events: SubmittableResult['events'], section: string, method: string) {
  const found = events.find(({ event }) => event.section === section && event.method === method);
  if (!found) {
    throw new Error(`Event ${section}.${method} not found among: ${JSON.stringify(events)}`);
  }
  return found.event.data;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function operatorsOf(planUpdatedEventData: any): [string, bigint][] {
  const operators = planUpdatedEventData.plan.operators;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return [...operators.entries()].map(([account, amount]: [any, any]) => [
    account.toString(),
    amount.toBigInt(),
  ]);
}

export async function testMultiDelegate(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await using polkadotApi = await getChainflipPolkadotApi();

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
  // we use chainflipPolkadotApi for submitting this extrinsic since dedot has a bug when there is a BTreeMap in the arguments of the extrinsic.
  const planUpdated = findEventData(
    await submitDelegateMulti(
      polkadotApi,
      delegator,
      new Map([
        [operatorA.address, amountToOperatorA],
        [operatorB.address, amountToOperatorB],
      ]),
    ),
    'validator',
    'DelegationPlanUpdated',
  );

  const operators = operatorsOf(planUpdated);
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
      `Unexpected delegation plan after delegate_multi: ${JSON.stringify(operators)}`,
    );
  }

  cf.info(`Updating delegation plan to only delegate to ${operatorA.address}...`);
  const updatedPlanUpdated = findEventData(
    await submitDelegateMulti(polkadotApi, delegator, new Map([[operatorA.address, totalAmount]])),
    'validator',
    'DelegationPlanUpdated',
  );

  const updatedOperators = operatorsOf(updatedPlanUpdated);
  if (
    updatedOperators.length !== 1 ||
    !updatedOperators.some(
      ([operator, amount]) => operator === operatorA.address && amount === totalAmount,
    )
  ) {
    throw new Error(
      `Unexpected delegation plan after switching to a single operator: ${JSON.stringify(updatedOperators)}`,
    );
  }

  cf.info('Undelegating from all operators via an empty plan...');
  const emptyPlanUpdated = findEventData(
    await submitDelegateMulti(polkadotApi, delegator, new Map()),
    'validator',
    'DelegationPlanUpdated',
  );

  if (operatorsOf(emptyPlanUpdated).length !== 0) {
    throw new Error(
      `Expected an empty delegation plan, got: ${JSON.stringify(operatorsOf(emptyPlanUpdated))}`,
    );
  }

  cf.info('Multi-operator delegation test completed successfully!');
}
