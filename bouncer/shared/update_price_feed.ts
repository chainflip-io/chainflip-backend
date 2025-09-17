import { Asset, Chain } from '@chainflip/cli';
import Web3 from 'web3';
import { signAndSendTxEvm } from 'shared/send_evm';
import { amountToFineAmount, getContractAddress, getEvmEndpoint } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { price as defaultPrice } from 'shared/setup_swaps';

// All price feeds are using 8 decimals
const PRICE_FEED_DECIMALS = 8;

async function updateEvmPriceFeed(logger: Logger, chain: Chain, asset: Asset, price: string) {
  const evmClient = new Web3(getEvmEndpoint(chain));
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
  await signAndSendTxEvm(logger, chain, priceFeedAddress, '0', txData);
}

export async function updatePriceFeed(logger: Logger, chain: Chain, asset: Asset, price: string) {
  if (!new Set(['BTC', 'ETH', 'SOL', 'USDC', 'USDT']).has(asset)) {
    throw new Error(`Unsupported price feed asset: ${asset}`);
  }

  switch (chain) {
    case 'Ethereum':
      await updateEvmPriceFeed(logger, 'Ethereum', asset, price);
      break;
    case 'Arbitrum':
      await updateEvmPriceFeed(logger, 'Arbitrum', asset, price);
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
  ]);
}
