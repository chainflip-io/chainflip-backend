import Web3 from 'web3';
import { getEvmEndpoint } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { initializeArbitrumChain, initializeArbitrumContracts } from './initialize_new_chains';
import { observeEvent } from './utils/substrate';

// This cuts out the pieces of arb activation from `bouncer/commands/setup_vaults.ts`
// So we can use it for the upgrade test.
export async function setupArbVault(): Promise<void> {
  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));

  // Step 1
  await initializeArbitrumChain();

  // Step 2
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());

  // Step 3
  const arbActivationRequest = observeEvent('arbitrumVault:AwaitingGovernanceActivation');

  const arbKey = (await arbActivationRequest).data.newPublicKey;

  // Step 4
  console.log('Inserting Arbitrum key in the contracts');
  await initializeArbitrumContracts(arbClient, arbKey);

  await submitGovernanceExtrinsic(async (api) =>
    api.tx.environment.witnessInitializeArbitrumVault(await arbClient.eth.getBlockNumber()),
  );

  console.log('Waiting for new epoch...');
  await observeEvent('validator:NewEpoch');

  console.log('=== New Epoch ===');
  console.log('=== Vault Setup completed ===');
}
