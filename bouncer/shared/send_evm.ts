import Web3 from 'web3';
import {
  Chain,
  amountToFineAmount,
  ethNonceMutex,
  arbNonceMutex,
  getEvmEndpoint,
  getWhaleKey,
  sleep,
  assetDecimals,
  getContractAddress,
} from './utils';
import { Logger } from './utils/logger';

const nextEvmNonce: { [key in 'Ethereum' | 'Arbitrum']: number | undefined } = {
  Ethereum: undefined,
  Arbitrum: undefined,
};

export async function getNextEvmNonce(chain: Chain): Promise<number> {
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
    if (nextEvmNonce[chain] === undefined) {
      const web3 = new Web3(getEvmEndpoint(chain));
      const whaleKey = getWhaleKey(chain);
      const address = web3.eth.accounts.privateKeyToAccount(whaleKey).address;
      const txCount = await web3.eth.getTransactionCount(address);
      nextEvmNonce[chain] = txCount;
    }

    return nextEvmNonce[chain]!++;
  });
}

export async function signAndSendTxEvm(
  logger: Logger,
  chain: Chain,
  to: string,
  value?: string,
  data?: string,
  gas = chain === 'Arbitrum' ? 5000000 : 200000,
) {
  const web3 = new Web3(getEvmEndpoint(chain));
  const whaleKey = getWhaleKey(chain);

  const nonce = await getNextEvmNonce(chain);
  const tx = { to, data, gas, nonce, value };

  const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);

  let receipt;
  const numberRetries = 10;

  // Retry mechanism as we expect all transactions to succeed.
  for (let i = 0; i < numberRetries; i++) {
    try {
      receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);
      break;
    } catch (error) {
      if (i === numberRetries - 1) {
        throw new Error(`${chain} transaction failure: ${error}`);
      }
      logger.error(`${chain} Retrying transaction. Found error: ${error}`);
    }
  }
  if (!receipt) {
    throw new Error('Receipt not found');
  }

  logger.trace(
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
  await signAndSendTxEvm(logger, chain, evmAddress, weiAmount, undefined, undefined);
}

export async function spamEvm(
  logger: Logger,
  chain: Chain,
  periodMilliSec: number,
  spam?: () => boolean,
) {
  const continueSpam = spam ?? (() => true);

  while (continueSpam()) {
    /* eslint-disable @typescript-eslint/no-floating-promises */
    signAndSendTxEvm(
      logger,
      chain,
      '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266',
      '1',
      undefined,
      undefined,
    );
    await sleep(periodMilliSec);
  }
}

const EVM_BASE_GAS_LIMIT = 21000;

export async function estimateCcmCfTesterGas(destChain: Chain, message: string) {
  const web3 = new Web3(getEvmEndpoint(destChain));
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
