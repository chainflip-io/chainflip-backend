import * as anchor from '@coral-xyz/anchor';
import { PublicKey } from '@solana/web3.js';
import {
  decodeSolAddress,
  getContractAddress,
  getSolConnection,
  tryUntilSuccess,
} from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { getSolanaVaultIdl } from '../shared/contract_interfaces';
import { Vault } from '../../contract-interfaces/sol-program-idls/v1.0.1-swap-endpoint/vault';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

type VaultSwapSettings = {
  minNativeSwapAmount: number;
  maxDstAddressLen: number;
  maxCcmMessageLen: number;
  maxCfParametersLen: number;
  maxEventAccounts: number;
  minTokenSwapAmount: number;
  tokenMintPubkey: PublicKey;
};

async function getVaultDataAccount() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const VaultIdl: any = await getSolanaVaultIdl();
  const cfVaultProgram = new anchor.Program<Vault>(
    VaultIdl as Vault,
    { connection: getSolConnection() } as anchor.Provider,
  );
  const vaultDataAccountAddress = new PublicKey(getContractAddress('Solana', 'DATA_ACCOUNT'));

  return cfVaultProgram.account.dataAccount.fetch(vaultDataAccountAddress);
}

async function getTokenSupportedAccount() {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const VaultIdl: any = await getSolanaVaultIdl();
  const cfVaultProgram = new anchor.Program<Vault>(
    VaultIdl as Vault,
    { connection: getSolConnection() } as anchor.Provider,
  );
  const vaultUsdcTokenSupportedAccount = new PublicKey(
    getContractAddress('Solana', 'SolUsdcTokenSupport'),
  );

  return cfVaultProgram.account.supportedToken.fetch(vaultUsdcTokenSupportedAccount);
}

async function submitNativeVaultSettingsGovernance(logger: Logger, settings: VaultSwapSettings) {
  const {
    minNativeSwapAmount,
    maxDstAddressLen,
    maxCcmMessageLen,
    maxCfParametersLen,
    maxEventAccounts,
  } = settings;

  logger.info('Submitting native vault settings via governance');
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.dispatchSolanaGovCall({
      SetProgramSwapsParameters: {
        minNativeSwapAmount,
        maxDstAddressLen,
        maxCcmMessageLen,
        maxCfParametersLen,
        maxEventAccounts,
      },
    }),
  );
}

async function submitTokenVaultSettingsGovernance(logger: Logger, settings: VaultSwapSettings) {
  const { minTokenSwapAmount, tokenMintPubkey } = settings;

  logger.info('Submitting token vault settings via governance');
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.dispatchSolanaGovCall({
      SetTokenSwapParameters: {
        minSwapAmount: minTokenSwapAmount,
        tokenMintPubkey: decodeSolAddress(tokenMintPubkey.toString()),
      },
    }),
  );
}

async function awaitVaultSettings(expectedSettings: VaultSwapSettings) {
  async function checkVaultSettings(): Promise<boolean> {
    const nativeSettings = await getVaultDataAccount();
    const tokenSettings = await getTokenSupportedAccount();

    const minNativeAmountMatches =
      nativeSettings.minNativeSwapAmount.toString() ===
      expectedSettings.minNativeSwapAmount.toString();

    const maxDstAddressLenMatches =
      nativeSettings.maxDstAddressLen.toString() === expectedSettings.maxDstAddressLen.toString();

    const maxCcmMessageLenMatches =
      nativeSettings.maxCcmMessageLen.toString() === expectedSettings.maxCcmMessageLen.toString();

    const maxCfParametersLenMatches =
      nativeSettings.maxCfParametersLen.toString() ===
      expectedSettings.maxCfParametersLen.toString();

    const maxEventAccountsMatches =
      nativeSettings.maxEventAccounts.toString() === expectedSettings.maxEventAccounts.toString();

    const minTokenSwapAmountMatches =
      tokenSettings.minSwapAmount.toString() === expectedSettings.minTokenSwapAmount.toString();

    const tokenMintPubkeyMatches =
      tokenSettings.tokenMintPubkey.toString() === expectedSettings.tokenMintPubkey.toString();

    return (
      minNativeAmountMatches &&
      maxDstAddressLenMatches &&
      maxCcmMessageLenMatches &&
      maxCfParametersLenMatches &&
      maxEventAccountsMatches &&
      minTokenSwapAmountMatches &&
      tokenMintPubkeyMatches
    );
  }
  await tryUntilSuccess(
    checkVaultSettings,
    6000,
    10,
    "Vault settings didn't match expected settings",
  );
}

export async function testSolanaVaultSettingsGovernance(testContext: TestContext) {
  // Initial settings
  let settings = {
    minNativeSwapAmount: 500000000,
    maxDstAddressLen: 64,
    maxCcmMessageLen: 10000,
    maxCfParametersLen: 1000,
    maxEventAccounts: 500,
    minTokenSwapAmount: 5000000,
    tokenMintPubkey: new PublicKey(getContractAddress('Solana', 'SolUsdc')),
  } as VaultSwapSettings;

  await awaitVaultSettings(settings);

  let newSettings = {
    minNativeSwapAmount: 100000000,
    maxDstAddressLen: 128,
    maxCcmMessageLen: 15000,
    maxCfParametersLen: 2000,
    maxEventAccounts: 1000,
    minTokenSwapAmount: settings.minTokenSwapAmount + 1,
    tokenMintPubkey: settings.tokenMintPubkey,
  };
  await submitNativeVaultSettingsGovernance(testContext.logger, newSettings);

  // Only the native settings should have changed
  settings = {
    minNativeSwapAmount: newSettings.minNativeSwapAmount,
    maxDstAddressLen: newSettings.maxDstAddressLen,
    maxCcmMessageLen: newSettings.maxCcmMessageLen,
    maxCfParametersLen: newSettings.maxCfParametersLen,
    maxEventAccounts: newSettings.maxEventAccounts,
    minTokenSwapAmount: settings.minTokenSwapAmount,
    tokenMintPubkey: settings.tokenMintPubkey,
  };
  await awaitVaultSettings(settings);

  newSettings = {
    minNativeSwapAmount: settings.minNativeSwapAmount + 1,
    maxDstAddressLen: settings.maxDstAddressLen + 1,
    maxCcmMessageLen: settings.maxCcmMessageLen + 1,
    maxCfParametersLen: settings.maxCfParametersLen + 1,
    maxEventAccounts: settings.maxEventAccounts + 1,
    minTokenSwapAmount: settings.minTokenSwapAmount + 10,
    tokenMintPubkey: settings.tokenMintPubkey,
  } as VaultSwapSettings;
  await submitTokenVaultSettingsGovernance(testContext.logger, newSettings);

  // Only the token settings should have changed
  settings = {
    minNativeSwapAmount: settings.minNativeSwapAmount,
    maxDstAddressLen: settings.maxDstAddressLen,
    maxCcmMessageLen: settings.maxCcmMessageLen,
    maxCfParametersLen: settings.maxCfParametersLen,
    maxEventAccounts: settings.maxEventAccounts,
    minTokenSwapAmount: newSettings.minTokenSwapAmount,
    tokenMintPubkey: newSettings.tokenMintPubkey,
  };
  await awaitVaultSettings(settings);
}
