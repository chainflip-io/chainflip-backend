import { HDNodeWallet, Wallet } from 'ethers';
import Web3 from 'web3';
import { InternalAsset as Asset, InternalAssets as Assets } from '@chainflip/cli';
import BigNumber from 'bignumber.js';
import { getBalance } from '../get_balance';
import { send } from '../send';
import { getEvmEndpoint, chainFromAsset, getWhaleMnemonic } from '../utils';

// Root whale key - private. This should never be used directly,
// since all it's funds are used in the WhaleKeyManager
export function getEvmRootWhaleKey(): string {
  return (
    process.env.ETH_USDC_WHALE ??
    '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80'
  );
}

// Tests should almost always be using this since instead of the above root key
export class WhaleKeyManager {
  private static keys: HDNodeWallet[] = [];

  private static currentIndex: number = 0;

  private static initialized: boolean = false;

  private static initializationPromise: Promise<void> | null = null;

  private static NUMBER_OF_WALLETS: number = 10;

  // Initialize for Ethereum and Arbitrum
  public static async initialize(): Promise<void> {
    // If already fully initialized, return immediately
    if (this.initialized) return Promise.resolve();
    // If initialization is in progress, wait for it
    if (this.initializationPromise) {
      return this.initializationPromise;
    }

    this.initializationPromise = (async (): Promise<void> => {
      const rootWallet = Wallet.fromPhrase(getWhaleMnemonic('Ethereum'));
      // Get all the balance of each asset from the root wallet

      const web3 = new Web3(getEvmEndpoint('Ethereum'));
      const arbWeb3 = new Web3(getEvmEndpoint('Arbitrum'));

      // store the balances in a map
      const balances = new Map<Asset, string>();
      const assets = [
        Assets.Flip,
        Assets.Usdc,
        Assets.Usdt,
        Assets.ArbUsdc,
        Assets.Eth,
        Assets.ArbEth,
      ];

      for (const asset of assets) {
        if (chainFromAsset(asset) === 'Ethereum') {
          const account = web3.eth.accounts.privateKeyToAccount(rootWallet.privateKey);
          const balance = await getBalance(asset, account.address);
          balances.set(asset, balance);
        } else if (chainFromAsset(asset) === 'Arbitrum') {
          const account = arbWeb3.eth.accounts.privateKeyToAccount(rootWallet.privateKey);
          const balance = await getBalance(asset, account.address);
          balances.set(asset, balance);
        }
      }

      console.log('Initializing whale key pool...');
      const promises = Array(this.NUMBER_OF_WALLETS)
        .fill(null)
        .map(async () => {
          const mnemonic = Wallet.createRandom().mnemonic?.phrase ?? '';
          if (mnemonic === '') {
            throw new Error('Failed to create random mnemonic');
          }
          const wallet = Wallet.fromPhrase(mnemonic);

          for (const asset of assets) {
            // divide the balance by the number of wallets
            const balance = new BigNumber(Math.floor(Number(balances.get(asset))));
            console.log(`Balance of whale key: ${balance}`);
            // divide by an extra one so the root wallet still has some funds - this is a workaround because
            // we run some bouncer tests in different scripts, otherwise we could always use this WhaleKeyManager
            const toSend = balance.div(this.NUMBER_OF_WALLETS + 1).toString();
            const toSendFloored = toSend.split('.')[0];
            console.log(
              `Whale key manager: Sending from root whale ${toSendFloored} ${asset} to ${wallet.address}`,
            );
            await send(asset, wallet.address, toSendFloored, true, rootWallet.privateKey);
            console.log(`Sent ${toSend} ${asset} to ${wallet.address}`);
          }

          console.log(`Wallet: ${wallet.address}`);
          return wallet;
        });

      this.keys = await Promise.all(promises);
      this.initialized = true;
      console.log('Whale key pool initialized with 10 funded accounts');
    })();

    return this.initializationPromise;
  }

  public static async getNextKey(): Promise<string> {
    await this.initialize();

    const key = this.keys[this.currentIndex].privateKey;
    this.currentIndex = (this.currentIndex + 1) % this.keys.length;
    return key;
  }
}
