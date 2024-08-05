import { getSolConnection } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { initializeSolanaChain, initializeSolanaPrograms } from './initialize_new_chains';
import { observeEvent } from './utils/substrate';

// This cuts out the pieces of arb activation from `bouncer/commands/setup_vaults.ts`
// So we can use it for the upgrade test.
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
  console.log('=== Solana Vault Setup completed ===');
}
