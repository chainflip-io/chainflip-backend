import Web3 from 'web3';
import { approveVault } from '@chainflip/cli';
import {
  Chain,
  amountToFineAmount,
  ethNonceMutex,
  arbNonceMutex,
  getEvmEndpoint,
  sleep,
  assetDecimals,
  getContractAddress,
} from './utils';

import { WhaleKeyManager } from './utils/whale_key_manager';

const nextEvmNonce: {
  [chain in 'Ethereum' | 'Arbitrum']: {
    [privateKey: string]: number;
  };
} = {
  Ethereum: {},
  Arbitrum: {},
};

export async function getNextEvmNonce(
  chain: Chain,
  privateKey: string,
  callback?: (nextNonce: number) => ReturnType<typeof approveVault>,
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
    if (nextEvmNonce[chain][privateKey] === undefined) {
      const web3 = new Web3(getEvmEndpoint(chain));
      const address = web3.eth.accounts.privateKeyToAccount(privateKey).address;
      const txCount = await web3.eth.getTransactionCount(address);
      nextEvmNonce[chain][privateKey] = txCount;
    }
    // The SDK returns null if no transaction is sent
    if (callback && (await callback(nextEvmNonce[chain][privateKey]!)) === null) {
      return nextEvmNonce[chain][privateKey]!;
    }
    return nextEvmNonce[chain][privateKey]!++;
  });
}

export async function signAndSendTxEvm(
  chain: Chain,
  to: string,
  value?: string,
  data?: string,
  privateKey?: string,
  gas = chain === 'Arbitrum' ? 5000000 : 200000,
  log = true,
) {
  if (!privateKey) {
    console.log('No private key provided, using next key');
  } else {
    console.log('Using private key provided: ', privateKey);
  }
  const sendingKey = privateKey ?? (await WhaleKeyManager.getNextKey());
  const nonce = await getNextEvmNonce(chain, sendingKey);
  console.log(`Nonce for key: ${sendingKey} is ${nonce}`);
  const tx = { to, data, gas, nonce, value };

  const web3 = new Web3(getEvmEndpoint(chain));
  const signedTx = await web3.eth.accounts.signTransaction(tx, sendingKey);

  let receipt;
  const numberRetries = 40;

  // Retry mechanism as we expect all transactions to succeed.
  for (let i = 0; i < numberRetries; i++) {
    try {
      receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction as string);
      break;
    } catch (error) {
      if (i === numberRetries - 1) {
        throw new Error(`${chain} transaction failure: ${error}`);
      }
      console.log(
        `${chain} Retrying transaction from key ${sendingKey} with nonce ${nonce}. Error: ${error}`,
      );
      await sleep(2000);
    }
  }
  if (!receipt) {
    throw new Error('Receipt not found');
  }

  if (log) {
    console.log(
      'Transaction complete, tx_hash: ' +
        receipt.transactionHash +
        ' blockNumber: ' +
        receipt.blockNumber +
        ' blockHash: ' +
        receipt.blockHash,
    );
  }
  return receipt;
}

export async function sendEvmNative(
  chain: Chain,
  evmAddress: string,
  ethAmount: string,
  log = true,
  privateKey?: string,
) {
  const weiAmount = amountToFineAmount(ethAmount, assetDecimals('Eth'));
  await signAndSendTxEvm(chain, evmAddress, weiAmount, undefined, privateKey, undefined, log);
}

export async function spamEvm(
  chain: Chain,
  privateKey: string,
  periodMilisec: number,
  spam?: () => boolean,
) {
  const continueSpam = spam ?? (() => true);

  while (continueSpam()) {
    /* eslint-disable @typescript-eslint/no-floating-promises */
    signAndSendTxEvm(
      chain,
      '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266',
      '1',
      undefined,
      privateKey,
    );
    await sleep(periodMilisec);
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

  // Estimate needs to be done using "from: vault" to prevent logic revertion
  return (await web3.eth.estimateGas({ data, to: cfTester, from: vault })) - EVM_BASE_GAS_LIMIT;
}
