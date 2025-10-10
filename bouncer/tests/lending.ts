import { Logger } from 'pino';
import assert from 'assert';
import { depositLiquidity, getFreeBalance } from 'shared/deposit_liquidity';
import { setupLpAccount } from 'shared/setup_account';
import {
  amountToFineAmount,
  amountToFineAmountBigInt,
  Asset,
  assetDecimals,
  ChainflipExtrinsicSubmitter,
  lpMutex,
} from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { randomBytes } from 'crypto';

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

async function getLoan(address: string) {
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

  // Change the interest interval to 1 block for testing
  logger.debug(`Setting interest payment interval to 1 block`);
  await submitGovernanceExtrinsic((api) =>
    api.tx.lendingPools.updatePalletConfig([{ SetInterestPaymentIntervalBlocks: 10 }]),
  );

  // Create a new random LP account
  const seed = randomBytes(4).toString('hex');
  const lpUri = `//LP_LENDING_${collateralAsset}_${loanAsset}_${seed}`;
  const lp = await setupLpAccount(logger, lpUri);
  const extrinsicSubmitter = new ChainflipExtrinsicSubmitter(lp, lpMutex.for(lpUri));

  // Add collateral to the account, a little extra to cover fees
  await depositLiquidity(logger, collateralAsset, collateralAmount * 1.1, true, lpUri);
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
      new Map(collateral.map(([asset, amount]) => [{ [asset]: {} }, amount])),
    ),
  );

  // Get balances before loan
  const collateralAssetFreeBalance1 = await getFreeBalance(lp.address, collateralAsset);
  const loanAssetFreeBalance1 = await getFreeBalance(lp.address, loanAsset);

  // Create a loan
  const loanDetails = await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.requestLoan(
      loanAsset,
      amountToFineAmount(loanAmount.toString(), assetDecimals(loanAsset)),
      collateralAsset,
      [],
    ),
  );
  logger.debug(`Created loan ${JSON.stringify(loanDetails)}`);

  // Check that our collateral is gone and we got the loan asset
  const collateralAssetFreeBalance2 = await getFreeBalance(lp.address, collateralAsset);
  const loanAssetFreeBalance2 = await getFreeBalance(lp.address, loanAsset);
  logger.debug(
    `Free Balances after loan: ${collateralAssetFreeBalance2} ${collateralAsset}, ${loanAssetFreeBalance2} ${loanAsset}`,
  );
  assert(
    collateralAssetFreeBalance2 - collateralAssetFreeBalance1 <
      amountToFineAmountBigInt(collateralAmount, collateralAsset),
    'Collateral balance did not decrease',
  );
  assert.strictEqual(
    loanAssetFreeBalance2 - loanAssetFreeBalance1,
    amountToFineAmountBigInt(loanAmount, loanAsset),
    'Loan balance did not increase correctly',
  );

  // Make sure the origination fee was taken
  const loanId = (await getLoan(lp.address)).loan_id;
  await observeEvent(logger, 'lendingPools:OriginationFeeTaken', {
    test: (event) => event.data.loan_id === loanId,
    historicalCheckBlocks: 15,
  });

  // Wait for some interest
  await observeEvent(logger, 'lendingPools:InterestTaken', {
    test: (event) => event.data.loan_id === loanId,
    timeoutSeconds: 15,
  });

  // Repay part of the loan
  logger.debug(`Repaying half the loan`);
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.makeRepayment(
      loanId,
      amountToFineAmount((loanAmount / 2).toString(), assetDecimals(loanAsset)),
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

  // Repay the rest of the loan
  logger.debug(`Repaying the rest of the loan`);
  const loanSettledEvent = observeEvent(logger, 'lendingPools:LoanSettled', {
    test: (event) => event.data.loan_id === loanId,
  });
  await extrinsicSubmitter.submit(
    chainflip.tx.lendingPools.makeRepayment(
      loanId,
      amountToFineAmount((loanAmount / 2).toString(), assetDecimals(loanAsset)),
    ),
  );
  await loanSettledEvent;

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
  const collateralAssetFreeBalance4 = await getFreeBalance(lp.address, collateralAsset);
  const loanAssetFreeBalance4 = await getFreeBalance(lp.address, loanAsset);
  assert(
    collateralAssetFreeBalance4 > collateralAssetFreeBalance2,
    'Did not get collateral back after we removed collateral',
  );
  assert(
    loanAssetFreeBalance4 < loanAssetFreeBalance3,
    'Did not lose loan asset after full repayment',
  );
}

export async function lendingTest() {
  await lendingTestForAsset(globalLogger, 'Eth', 35, 'Btc', 1.8);
}
