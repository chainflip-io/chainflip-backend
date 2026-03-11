import { Semaphore } from 'async-mutex';
import {
  Chain,
  amountToFineAmount,
  getWeb3,
  getEvmWhaleKeypair,
  assetDecimals,
  getContractAddress,
} from 'shared/utils';
import { KeyedMutex } from 'shared/utils/keyed_mutex';
import { Logger } from 'shared/utils/logger';

// Cap in-flight EVM transactions per chain to stay under geth's default txpool
// limit of 64 queued txs per account.
const MAX_IN_FLIGHT_TXS = 48;
const ethSemaphore = new Semaphore(MAX_IN_FLIGHT_TXS);
const arbSemaphore = new Semaphore(MAX_IN_FLIGHT_TXS);

// Nonce tracking and mutex per (chain, account) to avoid cross-account collisions.
const nonceMutex = new KeyedMutex('nonceMutex');
const nextEvmNonce = new Map<string, number>();

function nonceKey(chain: string, address: string): string {
  return `${chain}:${address.toLowerCase()}`;
}

export async function getNextEvmNonce(
  logger: Logger,
  chain: Chain,
  options: {
    forceRefetch?: boolean;
    privkey: string;
  },
): Promise<number> {
  if (chain !== 'Ethereum' && chain !== 'Arbitrum') {
    throw new Error('Invalid chain');
  }

  const web3 = getWeb3(chain);
  const address = web3.eth.accounts.privateKeyToAccount(options.privkey).address;
  const key = nonceKey(chain, address);

  return nonceMutex.for(key).runExclusive(async () => {
    if (!nextEvmNonce.has(key) || options.forceRefetch) {
      const txCount = await web3.eth.getTransactionCount(address, 'pending');
      nextEvmNonce.set(key, txCount);
    }
    const nonce = nextEvmNonce.get(key)!;
    logger.debug(`Nonce for ${chain} (${address}) is: ${nonce}`);
    nextEvmNonce.set(key, nonce + 1);
    return nonce;
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

export async function warnIfEvmAddressHasNoCode(logger: Logger, chain: Chain, address: string) {
  if (chain !== 'Ethereum' && chain !== 'Arbitrum') return;

  try {
    const web3 = getWeb3(chain);
    const code = await web3.eth.getCode(address);
    const hasNoCode = !code || code === '0x' || /^0x0+$/.test(code);
    if (hasNoCode) {
      logger.warn(
        `Address ${address} on ${chain} has no contract code (eth_getCode=${code}). Deposit may not be witnessed.`,
      );
    }
  } catch (err) {
    logger.warn(`Failed to check contract code for address ${address} on ${chain}: ${String(err)}`);
  }
}

async function getEvmTxRevertReason(chain: Chain, txHash: string): Promise<string> {
  const web3 = getWeb3(chain);
  const tx = await web3.eth.getTransaction(txHash);
  if (!tx.to || !tx.from) {
    return 'transaction details missing: to/from';
  }

  try {
    await web3.eth.call(
      {
        to: tx.to,
        from: tx.from,
        data: tx.input,
        value: tx.value,
      },
      tx.blockNumber as number,
    );
    return 'revert reason not available';
  } catch (err) {
    return extractWeb3ErrorMessage(err);
  }
}

export async function signAndSendTxEvm(
  logger: Logger,
  chain: Chain,
  tx: {
    to: string;
    value?: string;
    data?: string;
    gas?: number;
  },
  options: {
    privateKey?: string;
  } = {},
) {
  const { to, value, data } = tx;
  const gas = tx.gas ?? (chain === 'Arbitrum' ? 5000000 : 200000);
  const semaphore = chain === 'Arbitrum' ? arbSemaphore : ethSemaphore;

  return semaphore.runExclusive(async () => {
    const web3 = getWeb3(chain);
    const privkey = options.privateKey ?? getEvmWhaleKeypair('Ethereum').privkey;

    const numberRetries = 10;
    let receipt;

    // Fetch nonce and sign outside the loop; re-sign only if we get a nonce error.
    let nonce = await getNextEvmNonce(logger, chain, { privkey });
    let signedTx = await web3.eth.accounts.signTransaction(
      { to, data, gas, nonce, value },
      privkey,
    );

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
          nonce = await getNextEvmNonce(logger, chain, { privkey, forceRefetch: true });
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
    if (!receipt.status) {
      const revertReason = await getEvmTxRevertReason(chain, receipt.transactionHash);
      logger.warn(
        `${chain} transaction mined but failed. revertReason=${revertReason} receipt=${JSON.stringify(
          receipt,
        )}`,
      );
    }
    logger.debug(`Transaction receipt: ${JSON.stringify(receipt)}`);

    logger.debug(
      'Transaction complete, tx_hash: ' +
        receipt.transactionHash +
        ' blockNumber: ' +
        receipt.blockNumber +
        ' blockHash: ' +
        receipt.blockHash,
    );

    return receipt;
  });
}

export async function sendEvmNative(
  logger: Logger,
  chain: Chain,
  evmAddress: string,
  ethAmount: string,
) {
  const weiAmount = amountToFineAmount(ethAmount, assetDecimals('Eth'));
  return signAndSendTxEvm(logger, chain, { to: evmAddress, value: weiAmount });
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
