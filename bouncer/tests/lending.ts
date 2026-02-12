import assert from 'assert';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { AccountRole, setupAccount } from 'shared/setup_account';
import {
  amountToFineAmount,
  amountToFineAmountBigInt,
  assetDecimals,
  getFreeBalance,
  sleep,
  Asset,
  Assets,
} from 'shared/utils';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { randomBytes } from 'crypto';
import { TestContext } from 'shared/utils/test_context';
import { setupLendingPools } from 'shared/lending';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';

import { lendingPoolsCollateralAdded } from 'generated/events/lendingPools/collateralAdded';
import { lendingPoolsLoanCreated } from 'generated/events/lendingPools/loanCreated';
import { lendingPoolsLoanSettled } from 'generated/events/lendingPools/loanSettled';
import { lendingPoolsCollateralRemoved } from 'generated/events/lendingPools/collateralRemoved';

export interface Loan {
  loan_id: number;
  asset: Asset;
  created_at: number;
  principal_amount: string;
}

async function getLoanAccount(address: string) {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const loanAccounts = (await chainflip.rpc('cf_loan_accounts', address)) as any[];
  if (!loanAccounts) {
    throw new Error('Invalid loan accounts response');
  }
  assert.strictEqual(loanAccounts.length, 1, 'Expected one loan account');
  return loanAccounts[0];
}

async function getLoan(address: string): Promise<Loan> {
  const loanAccount = await getLoanAccount(address);
  assert.strictEqual(loanAccount.loans.length, 1, 'Expected one loan');
  return loanAccount.loans[0];
}

async function lendingTestForAsset<A = []>(
  parentCf: ChainflipIO<A>,
  collateralAsset: Asset,
  collateralAmount: number,
  loanAsset: Asset,
  loanAmount: number,
) {
  // Create a new random LP account
  const seed = randomBytes(4).toString('hex');
  const lpUri: `//${string}` = `//LP_LENDING_${collateralAsset}_${loanAsset}_${seed}`;

  // setup LP account
  const lp = await setupAccount(parentCf, lpUri, AccountRole.LiquidityProvider);

  // Setup cf with account and logger
  const cf = parentCf
    .with({ account: fullAccountFromUri(lpUri, 'LP') })
    .withChildLogger(`${JSON.stringify({ collateralAsset, loanAsset })}`);

  // Credit the account with the collateral and a little of the loan asset to be able to settle the loan.
  // We also need a little extra of both assets to cover the ingress fee.
  const loanAssetDecimals = assetDecimals(loanAsset);
  const factor = 10 ** loanAssetDecimals;
  const extraLoanAssetAmount = Math.round(0.01 * loanAmount * factor) / factor;
  await cf.all([
    (subcf) => depositLiquidity(subcf, loanAsset, extraLoanAssetAmount * 1.01),
    (subcf) => depositLiquidity(subcf, collateralAsset, collateralAmount * 1.05),
  ]);

  // Add collateral to the account
  const collateralAssetFreeBalance1 = await getFreeBalance(lp.address, collateralAsset);
  cf.debug(`Current free balance of collateral asset: ${collateralAssetFreeBalance1}`);
  cf.debug(`Adding collateral`);
  const collateral: [Asset, string][] = [
    [
      collateralAsset,
      amountToFineAmount(collateralAmount.toString(), assetDecimals(collateralAsset)),
    ],
  ];

  const collateralAddedEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.lendingPools.addCollateral(
        collateralAsset,
        new Map(collateral.map(([asset, amount]) => [{ [asset]: {} }, amount])),
      ),
    expectedEvent: {
      name: 'LendingPools.CollateralAdded',
      schema: lendingPoolsCollateralAdded.refine((event) => event.borrowerId === lp.address),
    },
  });
  cf.debug(
    `Collateral ${collateralAddedEvent.collateral} successfully added for LP: ${collateralAddedEvent.borrowerId}`,
  );

  // Check that our collateral is gone
  const collateralAssetFreeBalance2 = await getFreeBalance(lp.address, collateralAsset);
  assert(
    collateralAssetFreeBalance1 - collateralAssetFreeBalance2 >=
      amountToFineAmountBigInt(collateralAmount, collateralAsset),
    `Free balance of collateral asset did not decrease the expected amount after doing \`addCollateral\`, expected a decrease of at least ${amountToFineAmountBigInt(
      collateralAmount,
      collateralAsset,
    )} but got ${collateralAssetFreeBalance1 - collateralAssetFreeBalance2}`,
  );

  // Create a loan
  cf.debug(`Requesting loan of ${loanAmount} ${loanAsset}`);
  const loanAssetFreeBalance1 = await getFreeBalance(lp.address, loanAsset);

  const loanCreatedEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.lendingPools.requestLoan(
        loanAsset,
        amountToFineAmount(loanAmount.toString(), assetDecimals(loanAsset)),
        collateralAsset,
        [], // No extra collateral needed
      ),
    expectedEvent: {
      name: 'LendingPools.LoanCreated',
      schema: lendingPoolsLoanCreated.refine((event) => event.borrowerId === lp.address),
    },
  });

  const loanId = Number(loanCreatedEvent.loanId);

  cf.debug(`Created loan id: ${loanId}`);

  // Check that we got the loan amount
  const loanAssetFreeBalance2 = await getFreeBalance(lp.address, loanAsset);
  assert.strictEqual(
    loanAssetFreeBalance2 - loanAssetFreeBalance1,
    amountToFineAmountBigInt(loanAmount, loanAsset),
    'Free balance of loan asset did not increase as expected after loan creation',
  );

  // Make sure the origination fee was added to the loan amount
  const loan = await getLoan(lp.address);
  assert(loan !== undefined, 'Did not find a loan on the account');
  assert.strictEqual(loanId, loan.loan_id, `Loan ID does not match ${loanId} !== ${loan.loan_id}`);
  assert(
    BigInt(loan.principal_amount) > amountToFineAmountBigInt(loanAmount, loanAsset),
    'Loan amount did not increase due to origination fee',
  );

  // Wait for some interest
  await sleep(6000);
  await observeEvent(cf.logger, 'lendingPools:InterestTaken', {
    test: (event) => Number(event.data.loanId) === loanId,
    timeoutSeconds: 15,
  }).event;
  assert(
    (await getLoan(lp.address)).principal_amount > loan.principal_amount,
    `Loan amount did not increase due to interest, expected more than ${loan.principal_amount} ${loanAsset}`,
  );

  // Repay part of the loan
  cf.debug(`Repaying half the loan`);
  const partialRepaymentAmount = loanAmount / 2;
  await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.lendingPools.makeRepayment(loanId, {
        Exact: amountToFineAmount(partialRepaymentAmount.toString(), assetDecimals(loanAsset)),
      }),
  });

  // Check balances
  const collateralAssetFreeBalance3 = await getFreeBalance(lp.address, collateralAsset);
  const loanAssetFreeBalance3 = await getFreeBalance(lp.address, loanAsset);
  assert.strictEqual(
    collateralAssetFreeBalance3,
    collateralAssetFreeBalance2,
    'Expected free balance of collateral asset to not change yet',
  );
  assert(
    loanAssetFreeBalance3 < loanAssetFreeBalance2,
    'Did not lose loan asset after partial repayment',
  );

  // Repay the rest of the loan (with a bit extra to cover the origination fee and interest)
  assert(
    BigInt((await getLoan(lp.address)).principal_amount) <= loanAssetFreeBalance3,
    'Not enough free balance to fully repay the loan',
  );
  cf.debug(`Repaying the rest of the loan`);
  const loanSettledEvent = await cf.submitExtrinsic({
    extrinsic: (api) => api.tx.lendingPools.makeRepayment(loanId, 'Full'),
    expectedEvent: {
      name: 'LendingPools.LoanSettled',
      schema: lendingPoolsLoanSettled.refine((event) => Number(event.loanId) === loanId),
    },
  });
  cf.debug(`Loan successfully settled loanId: ${loanSettledEvent.loanId}`);

  // Recover the collateral
  const collateralAmountToRemove = (await getLoanAccount(lp.address)).collateral[0]
    .amount as string;
  const collateralToRemove: [Asset, string][] = [[collateralAsset, collateralAmountToRemove]];

  const collateralRemovedEvent = await cf.submitExtrinsic({
    extrinsic: (api) =>
      api.tx.lendingPools.removeCollateral(
        new Map(collateralToRemove.map(([asset, amount]) => [{ [asset]: {} }, amount])),
      ),
    expectedEvent: {
      name: 'LendingPools.CollateralRemoved',
      schema: lendingPoolsCollateralRemoved.refine((event) => event.borrowerId === lp.address),
    },
  });
  cf.debug(
    `Collateral ${collateralRemovedEvent.collateral} successfully removed for LP: ${collateralRemovedEvent.borrowerId}`,
  );

  // Check balances
  const collateralAssetFreeBalance5 = await getFreeBalance(lp.address, collateralAsset);
  const loanAssetFreeBalance4 = await getFreeBalance(lp.address, loanAsset);
  assert(
    collateralAssetFreeBalance5 > collateralAssetFreeBalance3,
    'Did not get collateral back after we removed collateral',
  );
  assert(
    loanAssetFreeBalance4 < loanAssetFreeBalance3,
    'Did not lose loan asset after full repayment',
  );
}

export async function lendingTest(testContext: TestContext): Promise<void> {
  const cf = await newChainflipIO(testContext.logger, []);

  // Check if the lending pool exists. This can be removed after the `upgrade_test` uses the new lending pool setup.
  await using chainflip = await getChainflipApi();
  /* eslint-disable @typescript-eslint/no-explicit-any */
  const btcPool: any = (
    await chainflip.query.lendingPools.generalLendingPools(Assets.Btc)
  ).toJSON();
  if (!btcPool) {
    cf.info('Btc lending pool not found, running setupLendingPools');
    await setupLendingPools(cf);
  }

  // Set lending config and whitelist
  cf.debug(`Setting interest payment interval to 1 block and threshold to 1 usd`);
  await submitGovernanceExtrinsic((api) =>
    api.tx.lendingPools.updatePalletConfig([
      { SetInterestPaymentIntervalBlocks: 1 },
      { SetInterestCollectionThresholdUsd: 1 },
    ]),
  );
  cf.debug(`Disabling lending pool whitelist`);
  await submitGovernanceExtrinsic((api) => api.tx.lendingPools.updateWhitelist('SetAllowAll'));

  // Run test
  await lendingTestForAsset(cf, Assets.Eth, 35, Assets.Btc, 1.8);
}
