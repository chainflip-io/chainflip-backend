import { Logger } from 'pino';
import assert from 'assert';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { setupLpAccount } from 'shared/setup_account';
import {
  amountToFineAmount,
  amountToFineAmountBigInt,
  Asset,
  assetDecimals,
  ChainflipExtrinsicSubmitter,
  lpMutex,
  getFreeBalance,
  sleep,
  submitExtrinsic,
} from 'shared/utils';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { randomBytes } from 'crypto';
import { TestContext } from 'shared/utils/test_context';

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

async function lendingTestForAsset(
  parentLogger: Logger,
  collateralAsset: Asset,
  collateralAmount: number,
  loanAsset: Asset,
  loanAmount: number,
) {
  const logger = parentLogger.child({ collateralAsset, loanAsset });
  await using chainflip = await getChainflipApi();

  // Create a new random LP account
  const seed = randomBytes(4).toString('hex');
  const lpUri = `//LP_LENDING_${collateralAsset}_${loanAsset}_${seed}`;
  const lp = await setupLpAccount(logger, lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  // Credit the account with the collateral and a little of the loan asset to be able to settle the loan.
  // We also need a little extra of both assets to cover the ingress fee.
  const loanAssetDecimals = assetDecimals(loanAsset);
  const factor = 10 ** loanAssetDecimals;
  const extraLoanAssetAmount = Math.round(0.01 * loanAmount * factor) / factor;
  await Promise.all([
    depositLiquidity(logger, loanAsset, extraLoanAssetAmount * 1.01, true, lpUri),
    depositLiquidity(logger, collateralAsset, collateralAmount * 1.05, true, lpUri),
  ]);

  // Add collateral to the account
  const collateralAssetFreeBalance1 = await getFreeBalance(lp.address, collateralAsset);
  logger.debug(`Current free balance of collateral asset: ${collateralAssetFreeBalance1}`);
  logger.debug(`Adding collateral`);
  const collateral: [Asset, string][] = [
    [
      collateralAsset,
      amountToFineAmount(collateralAmount.toString(), assetDecimals(collateralAsset)),
    ],
  ];
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.addCollateral(
      collateralAsset,
      new Map(collateral.map(([asset, amount]) => [{ [asset]: {} }, { Exact: amount }])),
    ),
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
  logger.debug(`Requesting loan of ${loanAmount} ${loanAsset}`);
  const loanAssetFreeBalance1 = await getFreeBalance(lp.address, loanAsset);

  const loanId = Number(
    (
      await submitExtrinsic(
        lpUri,
        chainflip,
        chainflip.tx.lendingPools.requestLoan(
          loanAsset,
          amountToFineAmount(loanAmount.toString(), assetDecimals(loanAsset)),
          collateralAsset,
          [], // No extra collateral needed
        ),
        'lendingPools:LoanCreated',
        logger,
      )
    ).data.loanId,
  );

  logger.debug(`Created loan id: ${loanId}`);

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
  await observeEvent(logger, 'lendingPools:InterestTaken', {
    test: (event) => Number(event.data.loanId) === loanId,
    timeoutSeconds: 15,
  });
  assert(
    (await getLoan(lp.address)).principal_amount > loan.principal_amount,
    `Loan amount did not increase due to interest, expected more than ${loan.principal_amount} ${loanAsset}`,
  );

  // Repay part of the loan
  logger.debug(`Repaying half the loan`);
  const partialRepaymentAmount = loanAmount / 2;
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.makeRepayment(
      loanId,
      amountToFineAmount(partialRepaymentAmount.toString(), assetDecimals(loanAsset)),
    ),
  );

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

  // Repay the rest of the loan (a bit extra to cover the origination fee and interest)
  assert(
    BigInt((await getLoan(lp.address)).principal_amount) <= loanAssetFreeBalance3,
    'Not enough free balance to fully repay the loan',
  );
  const repayFullyAmount = loanAmount - partialRepaymentAmount + extraLoanAssetAmount;
  assert(
    loanAssetFreeBalance3 >= amountToFineAmountBigInt(repayFullyAmount, loanAsset),
    'Missing loan asset before',
  );
  logger.debug(`Repaying the rest of the loan`);
  await submitExtrinsic(
    lpUri,
    chainflip,
    chainflip.tx.lendingPools.makeRepayment(
      loanId,
      amountToFineAmount(repayFullyAmount.toString(), assetDecimals(loanAsset)),
    ),
    'lendingPools:LoanSettled',
    logger,
  );

  // Recover the collateral
  const collateralAmountToRemove = (await getLoanAccount(lp.address)).collateral[0]
    .amount as string;
  const collateralToRemove: [Asset, string][] = [[collateralAsset, collateralAmountToRemove]];
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.removeCollateral(
      new Map(collateralToRemove.map(([asset, amount]) => [{ [asset]: {} }, amount])),
    ),
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
  // Change the interest interval to 1 block and the threshold to minimum for testing
  testContext.logger.debug(`Setting interest payment interval to 1 block and threshold to 1 usd`);
  await submitGovernanceExtrinsic((api) =>
    api.tx.lendingPools.updatePalletConfig([
      { SetInterestPaymentIntervalBlocks: 1 },
      { SetInterestCollectionThresholdUsd: 1 },
    ]),
  );

  await lendingTestForAsset(testContext.logger, 'Eth', 35, 'Btc', 1.8);
}
