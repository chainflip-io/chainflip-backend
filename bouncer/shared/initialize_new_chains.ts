import Web3 from 'web3';
import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
  LAMPORTS_PER_SOL,
} from '@solana/web3.js';
import {
  getContractAddress,
  getSolWhaleKeyPair,
  encodeSolAddress,
  solanaNumberOfNonces,
  solanaNumberOfAdditionalNonces,
} from '../shared/utils';
import { sendSol, signAndSendTxSol } from '../shared/send_sol';
import { getSolanaVaultIdl, getKeyManagerAbi } from '../shared/contract_interfaces';
import { signAndSendTxEvm } from '../shared/send_evm';
import { submitGovernanceExtrinsic } from './cf_governance';
import { observeEvent } from './utils/substrate';
import { Logger } from './utils/logger';

export async function initializeArbitrumChain(logger: Logger) {
  logger.info('Initializing Arbitrum');
  const arbInitializationRequest = observeEvent(logger, 'arbitrumVault:ChainInitialized').event;
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.arbitrumVault.initializeChain());
  await arbInitializationRequest;
}

export async function initializeSolanaChain(logger: Logger) {
  logger.info('Initializing Solana');
  const solInitializationRequest = observeEvent(logger, 'solanaVault:ChainInitialized').event;
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.solanaVault.initializeChain());
  await solInitializationRequest;
}

export async function initializeArbitrumContracts(
  logger: Logger,
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
  await signAndSendTxEvm(logger, 'Arbitrum', keyManagerAddress, '0', txData);
}

function numberToBuffer(bytes: number, number: number): Buffer {
  const buf = Buffer.alloc(bytes);
  if (bytes === 2) {
    buf.writeUInt16LE(number, 0);
  } else if (bytes === 4) {
    buf.writeUInt32LE(number, 0);
  } else {
    throw new Error('Unsupported byte length');
  }
  return buf;
}

function bigNumberToU64Buffer(number: bigint): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(number, 0);
  return buf;
}

export async function initializeSolanaPrograms(
  logger: Logger,
  solClient: Connection,
  solKey: string,
) {
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

  const solanaVaultProgramId = new PublicKey(getContractAddress('Solana', 'VAULT'));
  const dataAccount = new PublicKey(getContractAddress('Solana', 'DATA_ACCOUNT'));
  const whaleKeypair = getSolWhaleKeyPair();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const vaultIdl: any = await getSolanaVaultIdl();

  const initializeDiscriminatorString = vaultIdl.instructions.find(
    (instruction: { name: string }) => instruction.name === 'initialize',
  ).discriminator;
  const initializeDiscriminator = new Uint8Array(initializeDiscriminatorString.map(Number));

  const solKeyBuffer = Buffer.from(solKey.slice(2), 'hex');
  const newAggKey = new PublicKey(encodeSolAddress(solKey));
  const tokenVaultPda = new PublicKey(getContractAddress('Solana', 'TOKEN_VAULT_PDA'));
  const upgradeSignerPda = new PublicKey('H7G2avdmRSQyVxPcgZJPGXVCPhC61TMAKdvYBRF42zJ9');

  // Fund new Solana Agg key
  logger.info('Funding Solana new aggregate key:', newAggKey.toString());
  await sendSol(logger, solKey, '100');

  // Initialize Vault program
  let tx = new Transaction().add(
    new TransactionInstruction({
      data: Buffer.concat([
        Buffer.from(initializeDiscriminator.buffer),
        solKeyBuffer,
        whaleKeypair.publicKey.toBuffer(), // govkey
        tokenVaultPda.toBuffer(),
        Buffer.from([255]), // tokenVaultPda bump
        upgradeSignerPda.toBuffer(),
        Buffer.from([255]), // upgradeSignerPda bump
        Buffer.from([0]), // suspendedVault (false)
        Buffer.from([1]), // suspendedLegacySwaps (true)
        Buffer.from([0]), // suspendedEventSwaps (false)
        bigNumberToU64Buffer(5n * (BigInt(LAMPORTS_PER_SOL) / 10n)), // minNativeSwapAmount
        numberToBuffer(2, 64), // maxDstAddressLen
        numberToBuffer(4, 10000), // maxCcmMessageLen
        numberToBuffer(4, 1000), // maxCfParametersLen
        numberToBuffer(4, 500), // max_event_accounts
      ]),
      keys: [
        { pubkey: dataAccount, isSigner: false, isWritable: true },
        { pubkey: whaleKeypair.publicKey, isSigner: true, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: solanaVaultProgramId,
    }),
  );
  await signAndSendTxSol(logger, tx);

  // Set nonce authority to the new AggKey
  for (const [nonceNumber, prefix] of [
    [solanaNumberOfNonces, ''],
    [solanaNumberOfAdditionalNonces, '-add-nonce'],
  ]) {
    for (let i = 0; i < Number(nonceNumber); i++) {
      const seed = prefix + i.toString();
      const nonceAccountPubKey = await PublicKey.createWithSeed(
        whaleKeypair.publicKey,
        seed,
        SystemProgram.programId,
      );

      tx = new Transaction().add(
        SystemProgram.nonceAuthorize({
          noncePubkey: new PublicKey(nonceAccountPubKey),
          authorizedPubkey: whaleKeypair.publicKey,
          newAuthorizedPubkey: newAggKey,
        }),
      );
      await signAndSendTxSol(logger, tx);
    }
  }

  // Set Vault's upgrade authority to upgradeSignerPda and enable token support
  tx = new Transaction().add(
    createUpgradeAuthorityInstruction(
      solanaVaultProgramId,
      whaleKeypair.publicKey,
      upgradeSignerPda,
    ),
  );

  // Add token support
  const enableTokenSupportDiscriminatorString = vaultIdl.instructions.find(
    (instruction: { name: string }) => instruction.name === 'enable_token_support',
  ).discriminator;
  const enableTokenSupportDiscriminator = new Uint8Array(
    enableTokenSupportDiscriminatorString.map(Number),
  );

  const solUsdcMintPubkey = new PublicKey(getContractAddress('Solana', 'SolUsdc'));

  const tokenSupportedAccount = new PublicKey(getContractAddress('Solana', 'SolUsdcTokenSupport'));

  tx.add(
    new TransactionInstruction({
      data: Buffer.concat([
        Buffer.from(enableTokenSupportDiscriminator.buffer),
        bigNumberToU64Buffer(5n * 10n ** 6n), // minTokenSwapAmount
      ]),
      keys: [
        { pubkey: dataAccount, isSigner: false, isWritable: true },
        { pubkey: whaleKeypair.publicKey, isSigner: true, isWritable: false },
        { pubkey: tokenSupportedAccount, isSigner: false, isWritable: true },
        { pubkey: solUsdcMintPubkey, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: solanaVaultProgramId,
    }),
  );
  await signAndSendTxSol(logger, tx);

  // Set Governance authority to the new AggKey (State Chain)
  const setGovKeyWithGovKeyDiscriminatorString = vaultIdl.instructions.find(
    (instruction: { name: string }) => instruction.name === 'set_gov_key_with_gov_key',
  ).discriminator;
  const setGovKeyWithGovKeyDiscriminator = new Uint8Array(
    setGovKeyWithGovKeyDiscriminatorString.map(Number),
  );
  tx = new Transaction().add(
    new TransactionInstruction({
      data: Buffer.concat([
        Buffer.from(setGovKeyWithGovKeyDiscriminator.buffer),
        newAggKey.toBuffer(), // newGovKey
      ]),
      keys: [
        { pubkey: dataAccount, isSigner: false, isWritable: true },
        { pubkey: whaleKeypair.publicKey, isSigner: true, isWritable: false },
      ],
      programId: solanaVaultProgramId,
    }),
  );
  await signAndSendTxSol(logger, tx);
}
