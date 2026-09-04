import { AssetSymbol as Asset, ChainflipChain as Chain } from '@chainflip/utils/chainflip';
import { signAndSendTxEvm } from 'shared/send_evm';
import { amountToFineAmount, getContractAddress, getWeb3 } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { price as defaultPrice } from 'shared/setup_swaps';

// All price feeds are using 8 decimals
const PRICE_FEED_DECIMALS = 8;

const PRICE_FEED_ASSETS = ['BTC', 'ETH', 'SOL', 'USDC', 'USDT', 'TRX', 'BNB', 'DOT'] as const;

type PriceFeedAsset = (typeof PRICE_FEED_ASSETS)[number];

export const PRICE_FEED_CHAINS_BY_ASSET: Record<PriceFeedAsset, readonly Chain[]> = {
  BTC: ['Ethereum', 'Arbitrum', 'Bsc'],
  ETH: ['Ethereum', 'Arbitrum', 'Bsc'],
  SOL: ['Ethereum', 'Arbitrum', 'Bsc'],
  USDC: ['Ethereum', 'Arbitrum', 'Bsc'],
  USDT: ['Ethereum', 'Arbitrum', 'Bsc'],
  TRX: ['Bsc'],
  BNB: ['Bsc'],
  DOT: ['Bsc'],
};

function isPriceFeedAsset(asset: Asset): asset is PriceFeedAsset {
  return PRICE_FEED_ASSETS.includes(asset as PriceFeedAsset);
}

export function getPriceFeedChains(asset: Asset): readonly Chain[] {
  if (!isPriceFeedAsset(asset)) {
    throw new Error(`Unsupported price feed asset: ${asset}`);
  }

  return PRICE_FEED_CHAINS_BY_ASSET[asset];
}

async function updateEvmPriceFeed(logger: Logger, chain: Chain, asset: Asset, price: string) {
  const evmClient = getWeb3(chain);
  const priceFeedAddress = getContractAddress(chain, `PRICE_FEED_${asset}`);
  const finePrice = amountToFineAmount(price, PRICE_FEED_DECIMALS);

  // Not adding it in the contract interfaces folder because these are functions added in
  // our mock, while that interface is the real one.
  const PRICE_FEED_GOV_ABI = [
    {
      inputs: [
        {
          internalType: 'uint80',
          name: 'newRoundId',
          type: 'uint80',
        },
        {
          internalType: 'int256',
          name: 'newAnswer',
          type: 'int256',
        },
        {
          internalType: 'uint256',
          name: 'newStartedAt',
          type: 'uint256',
        },
        {
          internalType: 'uint256',
          name: 'newUpdatedAt',
          type: 'uint256',
        },
        {
          internalType: 'uint80',
          name: 'newAnsweredInRound',
          type: 'uint80',
        },
      ],
      name: 'submitRound',
      outputs: [],
      stateMutability: 'nonpayable',
      type: 'function',
    },
    {
      inputs: [
        {
          internalType: 'int256',
          name: 'newAnswer',
          type: 'int256',
        },
      ],
      name: 'updatePrice',
      outputs: [],
      stateMutability: 'nonpayable',
      type: 'function',
    },
    {
      inputs: [
        {
          internalType: 'uint8',
          name: 'newDecimals',
          type: 'uint8',
        },
        {
          internalType: 'uint256',
          name: 'newVersion',
          type: 'uint256',
        },
      ],
      name: 'updateSettings',
      outputs: [],
      stateMutability: 'nonpayable',
      type: 'function',
    },
  ];

  const priceFeedContract = new evmClient.eth.Contract(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    PRICE_FEED_GOV_ABI as any,
    priceFeedAddress,
  );
  const txData = priceFeedContract.methods.updatePrice(finePrice).encodeABI();
  await signAndSendTxEvm(logger, chain, { to: priceFeedAddress, value: '0', data: txData });
}

export async function updatePriceFeed(logger: Logger, chain: Chain, asset: Asset, price: string) {
  const supportedSourceChains = getPriceFeedChains(asset);

  if (!supportedSourceChains.includes(chain)) {
    throw new Error(
      `${asset} price feed is not configured on ${chain}. Configured source chains: ${supportedSourceChains.join(', ')}`,
    );
  }

  switch (chain) {
    case 'Ethereum':
    case 'Arbitrum':
    case 'Bsc':
      await updateEvmPriceFeed(logger, chain, asset, price);
      break;
    default:
      throw new Error(`Unsupported chain for price feed update: ${chain}`);
  }
}

export async function updateDefaultPriceFeeds(logger: Logger) {
  await Promise.all([
    updatePriceFeed(logger, 'Ethereum', 'BTC', defaultPrice.get('Btc')!.toString()),
    updatePriceFeed(logger, 'Ethereum', 'ETH', defaultPrice.get('Eth')!.toString()),
    updatePriceFeed(logger, 'Ethereum', 'SOL', defaultPrice.get('Sol')!.toString()),
    updatePriceFeed(logger, 'Ethereum', 'USDC', defaultPrice.get('Usdc')!.toString()),
    updatePriceFeed(logger, 'Ethereum', 'USDT', defaultPrice.get('Usdt')!.toString()),
    updatePriceFeed(logger, 'Arbitrum', 'BTC', defaultPrice.get('Btc')!.toString()),
    updatePriceFeed(logger, 'Arbitrum', 'ETH', defaultPrice.get('Eth')!.toString()),
    updatePriceFeed(logger, 'Arbitrum', 'SOL', defaultPrice.get('Sol')!.toString()),
    updatePriceFeed(logger, 'Arbitrum', 'USDC', defaultPrice.get('Usdc')!.toString()),
    updatePriceFeed(logger, 'Arbitrum', 'USDT', defaultPrice.get('Usdt')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'BTC', defaultPrice.get('Btc')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'ETH', defaultPrice.get('Eth')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'SOL', defaultPrice.get('Sol')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'USDC', defaultPrice.get('Usdc')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'USDT', defaultPrice.get('Usdt')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'TRX', defaultPrice.get('Trx')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'BNB', defaultPrice.get('Bnb')!.toString()),
    updatePriceFeed(logger, 'Bsc', 'DOT', defaultPrice.get('HubDot')!.toString()),
  ]);

  logger.info('All price feeds updated');
}
