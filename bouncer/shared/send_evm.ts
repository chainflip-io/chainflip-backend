import {
  arbNonceMutex,
  ethNonceMutex,
  Chain,
  amountToFineAmount,
  getWeb3,
  getEvmWhaleKeypair,
  assetDecimals,
  getContractAddress,
} from 'shared/utils';
import { Logger } from 'shared/utils/logger';

const nextEvmNonce: { [key in 'Ethereum' | 'Arbitrum']: number | undefined } = {
  Ethereum: undefined,
  Arbitrum: undefined,
};

export async function getNextEvmNonce(
  logger: Logger,
  chain: Chain,
  forceRefetch = false,
): Promise<number> {
  let mutex;
  switch (chain) {
    case 'Ethereum':
      mutex = ethNonceMutex;
      break;
    case 'Arbitrum':
      mutex = arbNonceMutex;
      break;
    default:
      throw new Error('Invalid chain');
  }

  return mutex.runExclusive(async () => {
    if (nextEvmNonce[chain] === undefined || forceRefetch) {
      const web3 = getWeb3(chain);
      const { privkey: whalePrivKey } = getEvmWhaleKeypair('Ethereum');
      const address = web3.eth.accounts.privateKeyToAccount(whalePrivKey).address;
      const txCount = await web3.eth.getTransactionCount(address, 'pending');
      // Only advance forward — never reset backwards into already-assigned nonces.
      if (nextEvmNonce[chain] === undefined || txCount > nextEvmNonce[chain]) {
        nextEvmNonce[chain] = txCount;
      }
    }
    logger.trace(`Nonce for ${chain} is: ${nextEvmNonce[chain]}`);
    return nextEvmNonce[chain]!++;
  });
}

// web3.js errors are sometimes plain objects { reason: Error, ... } rather than
// Error instances, so we extract the message from wherever it lives.
function extractWeb3ErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'object' && error !== null) {
    const e = error as Record<string, unknown>;
    if (typeof e.message === 'string') return e.message;
    if (e.reason instanceof Error) return e.reason.message;
    if (typeof e.reason === 'string') return e.reason;
  }
  return String(error);
}

function isEvmRevertError(error: unknown): boolean {
  const msg = extractWeb3ErrorMessage(error);
  return (
    msg.includes('Transaction has been reverted by the EVM') || msg.includes('execution reverted')
  );
}

function isNonceError(error: unknown): boolean {
  const msg = extractWeb3ErrorMessage(error);
  return msg.includes('nonce too low') || msg.includes('nonce too high');
}

export async function signAndSendTxEvm(
  logger: Logger,
  chain: Chain,
  to: string,
  value?: string,
  data?: string,
  gas = chain === 'Arbitrum' ? 5000000 : 200000,
  options: {
    privateKey?: string;
  } = {},
) {
  const web3 = getWeb3(chain);
  const privkey = options.privateKey ?? getEvmWhaleKeypair('Ethereum').privkey;

  const numberRetries = 10;
  let receipt;

  // Fetch nonce and sign outside the loop; re-sign only if we get a nonce error.
  let nonce = await getNextEvmNonce(logger, chain);
  let signedTx = await web3.eth.accounts.signTransaction({ to, data, gas, nonce, value }, privkey);

  for (let i = 0; i < numberRetries; i++) {
    try {
      receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);
      break;
    } catch (error) {
      // EVM reverts are deterministic — retrying the same tx will always fail.
      if (isEvmRevertError(error)) {
        throw new Error(`${chain} transaction reverted by EVM: ${error}`);
      }

      if (i === numberRetries - 1) {
        throw new Error(`${chain} transaction failure: ${error}`);
      }

      // On nonce errors, reset the counter so getNextEvmNonce re-fetches from chain
      // and re-sign with the corrected nonce.
      if (isNonceError(error)) {
        logger.warn(`${chain} nonce error, re-fetching nonce. Error: ${error}`);
        nonce = await getNextEvmNonce(logger, chain, true);
        signedTx = await web3.eth.accounts.signTransaction(
          { to, data, gas, nonce, value },
          privkey,
        );
      } else {
        logger.error(`${chain} Retrying transaction. Found error: ${error}`);
      }
    }
  }
  if (!receipt) {
    throw new Error('Receipt not found');
  }

  logger.debug(
    'Transaction complete, tx_hash: ' +
      receipt.transactionHash +
      ' blockNumber: ' +
      receipt.blockNumber +
      ' blockHash: ' +
      receipt.blockHash,
  );

  return receipt;
}

export async function sendEvmNative(
  logger: Logger,
  chain: Chain,
  evmAddress: string,
  ethAmount: string,
) {
  const weiAmount = amountToFineAmount(ethAmount, assetDecimals('Eth'));
  return signAndSendTxEvm(logger, chain, evmAddress, weiAmount, undefined, undefined);
}

const EVM_BASE_GAS_LIMIT = 21000;

export async function estimateCcmCfTesterGas(destChain: Chain, message: string) {
  const web3 = getWeb3(destChain);
  const cfTester = getContractAddress(destChain, 'CFTESTER');
  const vault = getContractAddress(destChain, 'VAULT');
  const messageLength = message.slice(2).length / 2;

  // Use a dummy valid call to the CfTester contract appending the actual message.
  const data =
    '0x4904ac5f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000e00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000' +
    web3.eth.abi.encodeParameters(['uint256'], [messageLength]).slice(2) +
    message.slice(2);

  // Estimate needs to be done using "from: vault" to prevent logic reversion
  return (await web3.eth.estimateGas({ data, to: cfTester, from: vault })) - EVM_BASE_GAS_LIMIT;
}
