import Web3 from 'web3';
import { assetDecimals, approveVault, Chain, Chains } from '@chainflip-io/cli';
import {
  amountToFineAmount,
  ethNonceMutex,
  arbNonceMutex,
  getEvmEndpoint,
  getWhaleKey,
} from './utils';

const nextEvmNonce: { [key in 'Ethereum' | 'Arbitrum']: number | undefined } = {
  Ethereum: undefined,
  Arbitrum: undefined,
};

export async function getNextEvmNonce(
  chain: Chain,
  callback?: (nextNonce: number) => ReturnType<typeof approveVault>,
): Promise<number> {
  let mutex;
  switch (chain) {
    case Chains.Ethereum:
      mutex = ethNonceMutex;
      break;
    case Chains.Arbitrum:
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
    // The SDK returns null if no transaction is sent
    if (callback && (await callback(nextEvmNonce[chain]!)) === null) {
      return nextEvmNonce[chain]!;
    }
    return nextEvmNonce[chain]!++;
  });
}

export async function signAndSendTxEvm(
  chain: Chain,
  to: string,
  value?: string,
  data?: string,
  gas = 2000000,
  log = true,
) {
  const web3 = new Web3(getEvmEndpoint(chain));
  const whaleKey = getWhaleKey(chain);

  const nonce = await getNextEvmNonce(chain);
  const tx = { to, data, gas, nonce, value };

  const signedTx = await web3.eth.accounts.signTransaction(tx, whaleKey);
  const receipt = await web3.eth.sendSignedTransaction(
    signedTx.rawTransaction as string,
    (error) => {
      if (error) {
        console.error('Ethereum transaction failure:', error);
      }
    },
  );

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
) {
  const weiAmount = amountToFineAmount(ethAmount, assetDecimals.ETH);
  await signAndSendTxEvm(chain, evmAddress, weiAmount, undefined, undefined, log);
}
