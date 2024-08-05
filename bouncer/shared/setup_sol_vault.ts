import { getSolConnection } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { initializeSolanaChain, initializeSolanaPrograms } from './initialize_new_chains';
import { observeEvent } from './utils/substrate';
import { createLpPool } from './create_lp_pool';
import { depositLiquidity } from './deposit_liquidity';
import { rangeOrder } from './range_order';

// This cuts out the pieces of sol activation from `bouncer/commands/setup_vaults.ts`
// so we can use it for the upgrade test.
export async function setupSolVault(): Promise<void> {
  const solClient = getSolConnection();

  // Step 1
  await initializeSolanaChain();

  // Step 2
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Step 3
  const solActivationRequest = observeEvent('solanaVault:AwaitingGovernanceActivation').event;

  const solKey = (await solActivationRequest).data.newPublicKey;

  // Step 4
  console.log('Inserting Solana key in the programs');
  await initializeSolanaPrograms(solClient, solKey);

  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.witnessInitializeSolanaVault(await solClient.getSlot()),
  );

  console.log('Waiting for new epoch...');
  await observeEvent('validator:NewEpoch').event;

  console.log('=== New Epoch ===');

  // Step 5
  console.log('=== Setting up for swaps ===');

  await Promise.all([createLpPool('Sol', 100), createLpPool('SolUsdc', 1)]);

  console.log('LP Pools created');

  await Promise.all([depositLiquidity('Sol', 1000), depositLiquidity('SolUsdc', 1000000)]);

  console.log('Liquidity provided');

  await Promise.all([rangeOrder('Sol', 1000 * 0.9999), rangeOrder('SolUsdc', 1000000 * 0.9999)]);

  console.log('Range orders placed');

  console.log('=== Swaps Setup completed ===');
}
