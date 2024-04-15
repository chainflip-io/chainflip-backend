import Web3 from 'web3';
import {
  getChainflipApi,
  getEvmContractAddress,
  getEvmEndpoint,
  observeEvent,
} from '../shared/utils';
import { getKeyManagerAbi } from '../shared/eth_abis';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { signAndSendTxEvm } from '../shared/send_evm';

// This cuts out the pieces of arb activation from `bouncer/commands/setup_vaults.ts`
// So we can use it for the upgrade test.
export async function setupArbVault(): Promise<void> {
  const chainflip = await getChainflipApi();

  const arbClient = new Web3(getEvmEndpoint('Arbitrum'));

  // Step 1
  console.log('Initializing Arbitrum');
  const arbInitializationRequest = observeEvent('arbitrumVault:ChainInitialized', chainflip);
  await submitGovernanceExtrinsic(chainflip.tx.arbitrumVault.initializeChain());
  await arbInitializationRequest;

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
  const keyManagerAddress = getEvmContractAddress('Arbitrum', 'KEY_MANAGER');
  const web3 = new Web3(getEvmEndpoint('Arbitrum'));

  const keyManagerContract = new web3.eth.Contract(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (await getKeyManagerAbi()) as any,
    keyManagerAddress,
  );
  const txData = keyManagerContract.methods
    .setAggKeyWithGovKey({
      pubKeyX: arbKey.pubKeyX,
      pubKeyYParity: arbKey.pubKeyYParity === 'Odd' ? 1 : 0,
    })
    .encodeABI();

  await signAndSendTxEvm('Arbitrum', keyManagerAddress, '0', txData);

  await submitGovernanceExtrinsic(
    chainflip.tx.environment.witnessInitializeArbitrumVault(await arbClient.eth.getBlockNumber()),
  );

  console.log('Waiting for new epoch...');
  await observeEvent('validator:NewEpoch', chainflip);

  console.log('=== New Epoch ===');
  console.log('=== Vault Setup completed ===');
  chainflip.disconnect();
}
