import Web3 from 'web3';
import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from '@solana/web3.js';
import { getContractAddress, getSolWhaleKeyPair, encodeSolAddress } from '../shared/utils';
import { signAndSendTxSol } from '../shared/send_sol';
import { getSolanaVaultIdl, getKeyManagerAbi } from '../shared/contract_interfaces';
import { signAndSendTxEvm } from '../shared/send_evm';
import { submitGovernanceExtrinsic } from './cf_governance';
import { observeEvent } from './utils/substrate';

export async function initializeArbitrumChain() {
  console.log('Initializing Arbitrum');
  const arbInitializationRequest = observeEvent('arbitrumVault:ChainInitialized').event;
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.arbitrumVault.initializeChain());
  await arbInitializationRequest;
}

export async function initializeArbitrumContracts(
  arbClient: Web3,
  arbKey: { pubKeyX: string; pubKeyYParity: string },
) {
  const keyManagerAddress = getContractAddress('Arbitrum', 'KEY_MANAGER');

  const keyManagerContract = new arbClient.eth.Contract(
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
}

export async function initializeSolanaPrograms(solClient: Connection, solKey: string) {
  function createUpgradeAuthorityInstruction(
    programId: PublicKey,
    upgradeAuthority: PublicKey,
    newUpgradeAuthority: PublicKey,
  ): TransactionInstruction {
    const BPF_UPGRADE_LOADER_ID = new PublicKey('BPFLoaderUpgradeab1e11111111111111111111111');
    const [programDataAddress] = PublicKey.findProgramAddressSync(
      [programId.toBuffer()],
      BPF_UPGRADE_LOADER_ID,
    );

    const keys = [
      {
        pubkey: programDataAddress,
        isWritable: true,
        isSigner: false,
      },
      {
        pubkey: upgradeAuthority,
        isWritable: false,
        isSigner: true,
      },
      {
        pubkey: newUpgradeAuthority,
        isWritable: false,
        isSigner: false,
      },
    ];
    return new TransactionInstruction({
      keys,
      programId: BPF_UPGRADE_LOADER_ID,
      data: Buffer.from([4, 0, 0, 0]), // SetAuthority instruction bincode
    });
  }

  // Temporal workaround if running the bouncer without the Solana node
  try {
    await solClient.getGenesisHash();
  } catch (e) {
    console.log('Solana not running, skipping key initialization');
    return;
  }

  console.log('Initializing Solana programs');

  const solanaVaultProgramId = new PublicKey(getContractAddress('Solana', 'VAULT'));
  const solanaUpgradeManagerProgramId = new PublicKey(
    getContractAddress('Solana', 'UPGRADE_MANAGER'),
  );
  const solanaUpgradeManagerSignerProgramId = new PublicKey(
    getContractAddress('Solana', 'UPGRADE_MANAGER_SIGNER'),
  );
  const dataAccount = new PublicKey(getContractAddress('Solana', 'DATA_ACCOUNT'));
  const whaleKeypair = getSolWhaleKeyPair();
  const vaultIdl = await getSolanaVaultIdl();

  const discriminatorString = vaultIdl.instructions.find(
    (instruction: { name: string }) => instruction.name === 'initialize',
  ).discriminator;
  const discriminator = new Uint8Array(discriminatorString.map(Number));

  const solKeyBuffer = Buffer.from(solKey.slice(2), 'hex');
  const newAggKey = new PublicKey(encodeSolAddress(solKey));
  const tokenVaultPda = new PublicKey(getContractAddress('Solana', 'TOKEN_VAULT_PDA'));

  // Initialize Vault program
  const tx = new Transaction().add(
    new TransactionInstruction({
      data: Buffer.concat([
        Buffer.from(discriminator.buffer),
        solKeyBuffer,
        solKeyBuffer,
        tokenVaultPda.toBuffer(),
        Buffer.from([255]),
      ]),
      keys: [
        { pubkey: dataAccount, isSigner: false, isWritable: true },
        { pubkey: whaleKeypair.publicKey, isSigner: true, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: solanaVaultProgramId,
    }),
  );

  // Set nonce authority to the new AggKey
  const numberOfNonceAccounts = 7;
  for (let i = 0; i < numberOfNonceAccounts; i++) {
    // Using the index stringified as the seed ('0', '1', '2' ...)
    const seed = i.toString();

    // Deriving the nonceAccounts with index seeds to find the nonce accounts
    const nonceAccountPubKey = await PublicKey.createWithSeed(
      whaleKeypair.publicKey,
      seed,
      SystemProgram.programId,
    );

    // Set nonce authority to the new AggKey
    tx.add(
      SystemProgram.nonceAuthorize({
        noncePubkey: new PublicKey(nonceAccountPubKey),
        authorizedPubkey: whaleKeypair.publicKey,
        newAuthorizedPubkey: newAggKey,
      }),
    );
  }
  // Set Vault's upgrade authority to Upgrade manager's PDA
  tx.add(
    createUpgradeAuthorityInstruction(
      solanaVaultProgramId,
      whaleKeypair.publicKey,
      solanaUpgradeManagerSignerProgramId,
    ),
  );
  // Set Upgrade Manager's upgrade authority to AggKey
  tx.add(
    createUpgradeAuthorityInstruction(
      solanaUpgradeManagerProgramId,
      whaleKeypair.publicKey,
      newAggKey,
    ),
  );
  await signAndSendTxSol(tx);
}
