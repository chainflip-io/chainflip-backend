import Web3 from 'web3';
import { assetDecimals, approveVault } from '@chainflip-io/cli';
import { amountToFineAmount, ethNonceMutex } from './utils';

let nextNonce: number | undefined;

export async function getNextEthNonce(
  callback?: (nextNonce: number) => ReturnType<typeof approveVault>,
): Promise<number> {
  return ethNonceMutex.runExclusive(async () => {
    if (nextNonce === undefined) {
      const ethEndpoint = process.env.ETH_ENDPOINT || 'http://127.0.0.1:8545';
      const web3 = new Web3(ethEndpoint);
      const whaleKey =
        process.env.ETH_USDC_WHALE ||
        '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
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

export async function signAndSendTxEth(
  to: string,
  value?: string,
  data?: string,
  gas = 2000000,
  log = true,
) {
  const ethEndpoint = process.env.ETH_ENDPOINT ?? 'http://127.0.0.1:8545';
  const web3 = new Web3(ethEndpoint);

  const whaleKey =
    process.env.ETH_USDC_WHALE ||
    '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';

  const nonce = await getNextEthNonce();
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

export async function sendEth(ethereumAddress: string, ethAmount: string, log = true) {
  const weiAmount = amountToFineAmount(ethAmount, assetDecimals.ETH);
  await signAndSendTxEth(ethereumAddress, weiAmount, undefined, undefined, log);
}
