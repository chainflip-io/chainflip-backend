import Web3 from 'web3';
import { getChainflipApi, getEvmEndpoint, observeEvent } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { initializeArbitrumChain, initializeArbitrumContracts } from './initialize_new_chains';

// This cuts out the pieces of arb activation from `bouncer/commands/setup_vaults.ts`
// So we can use it for the upgrade test.
export async function setupArbVault(): Promise<void> {
  const chainflip = await getChainflipApi();

  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));

  // Step 1
  await initializeArbitrumChain(chainflip);

  // Step 2
  console.log('Forcing rotation');
  await submitGovernanceExtrinsic(chainflip.tx.validator.forceRotation());

  // Step 3
  const arbActivationRequest = observeEvent(
    'arbitrumVault:AwaitingGovernanceActivation',
    chainflip,
  );

  const arbKey = (await arbActivationRequest).data.newPublicKey;

  // Step 4
  console.log('Inserting Arbitrum key in the contracts');
  await initializeArbitrumContracts(arbClient, arbKey);

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessInitializeArbitrumVault(await arbClient.eth.getBlockNumber()),
  );

  console.log('Waiting for new epoch...');
  await observeEvent('validator:NewEpoch', chainflip);

  console.log('=== New Epoch ===');
  console.log('=== Vault Setup completed ===');
}
