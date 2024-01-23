import Web3 from 'web3';
import { assetDecimals, approveVault, Chain } from '@chainflip-io/cli';
import { amountToFineAmount, ethNonceMutex, arbNonceMutex } from './utils';

let nextNonce: number | undefined;

export async function getNextEvmNonce(
  chain: Chain,
  callback?: (nextNonce: number) => ReturnType<typeof approveVault>,
): Promise<number> {
  const mutex = chain === 'Ethereum' ? ethNonceMutex : arbNonceMutex;

  return mutex.runExclusive(async () => {
    if (nextNonce === undefined) {
      const evmEndpoint =
        chain === 'Ethereum'
          ? process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'
          : process.env.ARB_ENDPOINT ?? 'http://127.0.0.1:8547';
      const web3 = new Web3(evmEndpoint);
      const whaleKey =
        chain === 'Ethereum'
          ? process.env.ETH_USDC_WHALE ||
            '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80'
          : process.env.ARB_WHALE ||
            '0xb6b15c8cb491557369f3c7d2c287b053eb229daa9c22138887752191c9520659';
      const address = web3.eth.accounts.privateKeyToAccount(whaleKey).address;
      const txCount = await web3.eth.getTransactionCount(address);
      nextNonce = txCount;
    }
    // The SDK returns null if no transaction is sent
    if (callback && (await callback(nextNonce)) === null) {
      return nextNonce;
    }
    return nextNonce++;
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
  const evmEndpoint =
    chain === 'Ethereum'
      ? process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545'
      : process.env.ARB_ENDPOINT ?? 'http://127.0.0.1:8547';

  const web3 = new Web3(evmEndpoint);

  const whaleKey =
    chain === 'Ethereum'
      ? process.env.ETH_USDC_WHALE ||
        '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80'
      : process.env.ARB_WHALE ||
        '0xb6b15c8cb491557369f3c7d2c287b053eb229daa9c22138887752191c9520659';

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
