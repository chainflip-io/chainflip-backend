import assert from 'assert';
import { lpApiRpc } from 'shared/json_rpc';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import {
  amountToFineAmountBigInt,
  amountToFineHex,
  Assets,
  createStateChainKeypair,
  getFreeBalance,
  stateChainAssetFromAsset,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { setupLendingPools } from 'shared/lending';

import { lendingPoolsLendingFundsAdded } from 'generated/events/lendingPools/lendingFundsAdded';
import { lendingPoolsLoanCreated } from 'generated/events/lendingPools/loanCreated';
import { lendingPoolsLoanUpdated } from 'generated/events/lendingPools/loanUpdated';
import { lendingPoolsLoanRepaid } from 'generated/events/lendingPools/loanRepaid';
import { lendingPoolsLoanSettled } from 'generated/events/lendingPools/loanSettled';
import { lendingPoolsLendingFundsRemoved } from 'generated/events/lendingPools/lendingFundsRemoved';
import { provideLiquidityAndTestAssetBalances } from './lp_api_test';

// Note: use a different asset to the other LP_API test to avoid conflicts when depositing the liquidity.
// Localnet oracle prices: SOL ~ $100, BTC ~ $10,000. Target LTV cap is 80%.
// 550 SOL ≈ $55k collateral; 1.5 BTC peak loan ≈ $15k → ~27% LTV.
const COLLATERAL_ASSET = Assets.Sol;
const COLLATERAL_AMOUNT = 500;
const COLLATERAL_FUNDS_ADDED = 550;
const LOAN_ASSET = Assets.Btc;
const LOAN_AMOUNT = 1;
const EXPAND_AMOUNT = 0.5;

const collateralRpcAsset = stateChainAssetFromAsset(COLLATERAL_ASSET);
const loanRpcAsset = stateChainAssetFromAsset(LOAN_ASSET);

async function ensureLendingPoolsReady<A>(cf: ChainflipIO<A>) {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const pool: any = (await chainflip.query.lendingPools.generalLendingPools(LOAN_ASSET)).toJSON();
  if (!pool) {
    cf.info(`${LOAN_ASSET} lending pool not found, running setupLendingPools`);
    await setupLendingPools(cf);
  }
}

async function getLoanAccount(lpAddress: string) {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const accounts = (await chainflip.rpc('cf_loan_accounts', lpAddress)) as any[];
  if (!accounts || accounts.length === 0) {
    return undefined;
  }
  return accounts[0];
}

async function testAddLenderFunds<A>(cf: ChainflipIO<A>, lpAddress: string) {
  const collateralFine = amountToFineAmountBigInt(COLLATERAL_FUNDS_ADDED, COLLATERAL_ASSET);
  const balanceBefore = await getFreeBalance(lpAddress, COLLATERAL_ASSET);

  const result = await lpApiRpc(cf.logger, 'lp_add_lender_funds', [
    collateralRpcAsset,
    amountToFineHex(COLLATERAL_FUNDS_ADDED, COLLATERAL_ASSET),
    'InBlock',
  ]);
  assert(result.tx_details, 'Expected tx_details for InBlock add_lender_funds');
  assert.strictEqual(
    result.tx_details.response,
    null,
    'Expected null response from lp_add_lender_funds',
  );

  await cf.stepToTransactionIncluded({
    hash: result.tx_details.tx_hash,
    expectedEvent: {
      name: 'LendingPools.LendingFundsAdded',
      schema: lendingPoolsLendingFundsAdded.refine(
        (event) =>
          event.lenderId === lpAddress &&
          event.asset === COLLATERAL_ASSET &&
          BigInt(event.amount) === collateralFine,
      ),
    },
  });

  const balanceAfter = await getFreeBalance(lpAddress, COLLATERAL_ASSET);
  assert(
    balanceBefore - balanceAfter >= collateralFine,
    `Free balance of ${COLLATERAL_ASSET} did not decrease by at least ${collateralFine}`,
  );
}

async function testRequestLoan<A>(cf: ChainflipIO<A>, lpAddress: string): Promise<number> {
  const loanFine = amountToFineAmountBigInt(LOAN_AMOUNT, LOAN_ASSET);
  const balanceBefore = await getFreeBalance(lpAddress, LOAN_ASSET);

  const result = await lpApiRpc(cf.logger, 'lp_request_loan', [
    loanRpcAsset,
    amountToFineHex(LOAN_AMOUNT, LOAN_ASSET),
    null,
    'InBlock',
  ]);
  const loanId = Number(result.tx_details.response);
  cf.debug(`Loan ID: ${loanId}`);

  await cf.stepToTransactionIncluded({
    hash: result.tx_details.tx_hash,
    expectedEvent: {
      name: 'LendingPools.LoanCreated',
      schema: lendingPoolsLoanCreated.refine((e) => e.loanId === BigInt(loanId)),
    },
  });

  const balanceAfter = await getFreeBalance(lpAddress, LOAN_ASSET);
  assert.strictEqual(
    balanceAfter - balanceBefore,
    loanFine,
    `Free balance of ${LOAN_ASSET} did not increase by exactly ${loanFine}`,
  );

  const loan = await getLoanAccount(lpAddress);
  assert(loan?.loans?.length > 0, 'No loan found on LP account after request_loan');

  return loanId;
}

async function testExpandLoan<A>(cf: ChainflipIO<A>, lpAddress: string, loanId: number) {
  const loanAccountBefore = await getLoanAccount(lpAddress);
  const principalBefore = BigInt(loanAccountBefore.loans.at(-1).principal_amount);

  const expandFine = amountToFineAmountBigInt(EXPAND_AMOUNT, LOAN_ASSET);
  const txHash = await lpApiRpc(cf.logger, 'lp_expand_loan', [
    loanId,
    amountToFineHex(EXPAND_AMOUNT, LOAN_ASSET),
  ]);
  assert.match(txHash, /^0x[0-9a-fA-F]{64}$/, `Unexpected expand_loan tx hash: ${txHash}`);

  await cf.stepToTransactionIncluded({
    hash: txHash,
    expectedEvent: {
      name: 'LendingPools.LoanUpdated',
      schema: lendingPoolsLoanUpdated.refine((e) => e.loanId === BigInt(loanId)),
    },
  });

  const loanAccountAfter = await getLoanAccount(lpAddress);
  const principalAfter = BigInt(loanAccountAfter.loans.at(-1).principal_amount);
  assert(
    principalAfter >= principalBefore + expandFine,
    `Principal did not increase by at least ${expandFine}: ${principalBefore} -> ${principalAfter}`,
  );
}

async function testMakeRepaymentExact<A>(cf: ChainflipIO<A>, lpAddress: string, loanId: number) {
  const result = await lpApiRpc(cf.logger, 'lp_make_repayment', [
    loanId,
    { Exact: amountToFineHex(LOAN_AMOUNT / 2, LOAN_ASSET) },
    'InBlock',
  ]);
  const response = result.tx_details.response;
  assert.strictEqual(response.is_settled, false, 'Expected partial repayment to not settle loan');
  assert(BigInt(response.amount) > 0n, 'Expected non-zero repayment amount');

  await cf.stepToTransactionIncluded({
    hash: result.tx_details.tx_hash,
    expectedEvent: {
      name: 'LendingPools.LoanRepaid',
      schema: lendingPoolsLoanRepaid.refine((e) => e.loanId === BigInt(loanId)),
    },
  });

  const loan = await getLoanAccount(lpAddress);
  assert(loan?.loans?.length > 0, 'Loan should still exist after partial repayment');
}

async function testMakeRepaymentFull<A>(cf: ChainflipIO<A>, lpAddress: string, loanId: number) {
  const result = await lpApiRpc(cf.logger, 'lp_make_repayment', [loanId, 'Full', 'InBlock']);
  const response = result.tx_details.response;
  assert.strictEqual(response.is_settled, true, 'Expected full repayment to settle loan');
  assert(BigInt(response.amount) > 0n, 'Expected non-zero repayment amount');

  await cf.stepToTransactionIncluded({
    hash: result.tx_details.tx_hash,
    expectedEvent: {
      name: 'LendingPools.LoanSettled',
      schema: lendingPoolsLoanSettled.refine((e) => e.loanId === BigInt(loanId)),
    },
  });

  const account = await getLoanAccount(lpAddress);
  assert(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    !account || !account.loans.some((loan: any) => loan.loanId === BigInt(loanId)),
    `Loan ${loanId} should have been removed from account after full repayment`,
  );
}

async function testRemoveLenderFunds<A>(cf: ChainflipIO<A>, lpAddress: string) {
  const balanceBefore = await getFreeBalance(lpAddress, COLLATERAL_ASSET);

  // Withdraw all (None)
  const result = await lpApiRpc(cf.logger, 'lp_remove_lender_funds', [
    collateralRpcAsset,
    null,
    'InBlock',
  ]);
  const unlockedAmount = BigInt(result.tx_details.response);
  assert(unlockedAmount > 0n, `Expected non-zero unlocked amount, got ${unlockedAmount}`);

  await cf.stepToTransactionIncluded({
    hash: result.tx_details.tx_hash,
    expectedEvent: {
      name: 'LendingPools.LendingFundsRemoved',
      schema: lendingPoolsLendingFundsRemoved.refine(
        (e) =>
          e.lenderId === lpAddress &&
          e.asset === COLLATERAL_ASSET &&
          BigInt(e.unlockedAmount) === unlockedAmount,
      ),
    },
  });

  const balanceAfter = await getFreeBalance(lpAddress, COLLATERAL_ASSET);
  assert(
    balanceAfter > balanceBefore,
    'Expected free balance of collateral asset to increase after withdrawing supply',
  );
}

async function getVoluntaryLiquidationFlag(lpAddress: string): Promise<boolean | undefined> {
  await using chainflip = await getChainflipApi();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const account = (await chainflip.query.lendingPools.loanAccounts(lpAddress)).toJSON() as any;
  return account?.voluntaryLiquidationRequested;
}

async function testVoluntaryLiquidation<A>(cf: ChainflipIO<A>, lpAddress: string) {
  // Set up a fresh loan to exercise voluntary liquidation
  await lpApiRpc(cf.logger, 'lp_add_lender_funds', [
    collateralRpcAsset,
    amountToFineHex(COLLATERAL_AMOUNT, COLLATERAL_ASSET),
    'InBlock',
  ]);

  const loanResult = await lpApiRpc(cf.logger, 'lp_request_loan', [
    loanRpcAsset,
    amountToFineHex(LOAN_AMOUNT, LOAN_ASSET),
    null,
    'InBlock',
  ]);
  const loanId = Number(loanResult.tx_details.response);

  // Initiate voluntary liquidation
  const initHash = await lpApiRpc(cf.logger, 'lp_initiate_voluntary_liquidation', []);
  assert.match(initHash, /^0x[0-9a-fA-F]{64}$/, `Unexpected initiate tx hash: ${initHash}`);
  await cf.stepToTransactionIncluded({ hash: initHash });

  assert.strictEqual(
    await getVoluntaryLiquidationFlag(lpAddress),
    true,
    'Expected voluntary_liquidation_requested to be true after initiate',
  );

  // Stop voluntary liquidation
  const stopHash = await lpApiRpc(cf.logger, 'lp_stop_voluntary_liquidation', []);
  assert.match(stopHash, /^0x[0-9a-fA-F]{64}$/, `Unexpected stop tx hash: ${stopHash}`);
  await cf.stepToTransactionIncluded({ hash: stopHash });

  assert.strictEqual(
    await getVoluntaryLiquidationFlag(lpAddress),
    false,
    'Expected voluntary_liquidation_requested to be false after stop',
  );

  // Clean up: settle the loan and recover collateral
  await lpApiRpc(cf.logger, 'lp_make_repayment', [loanId, 'Full', 'InBlock']);
  await lpApiRpc(cf.logger, 'lp_remove_lender_funds', [collateralRpcAsset, null, 'InBlock']);
}

export async function testLpApiLending(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  const lpAddress = createStateChainKeypair('//LP_API').address; // cFJt3kyUdXvaoarfxJDLrFmHFqkXUgnVZ4zqqDLLTRjbJosmK

  await provideLiquidityAndTestAssetBalances(cf, COLLATERAL_ASSET, COLLATERAL_AMOUNT * 2);

  await ensureLendingPoolsReady(cf);

  await testAddLenderFunds(cf.withChildLogger('testAddLenderFunds'), lpAddress);
  const loanId = await testRequestLoan(cf.withChildLogger('testRequestLoan'), lpAddress);
  await testExpandLoan(cf.withChildLogger('testExpandLoan'), lpAddress, loanId);
  await testMakeRepaymentExact(cf.withChildLogger('testMakeRepaymentExact'), lpAddress, loanId);
  await testMakeRepaymentFull(cf.withChildLogger('testMakeRepaymentFull'), lpAddress, loanId);
  await testRemoveLenderFunds(cf.withChildLogger('testRemoveLenderFunds'), lpAddress);
  await testVoluntaryLiquidation(cf.withChildLogger('testVoluntaryLiquidation'), lpAddress);
}
