import { Asset } from '@chainflip/cli';
import Web3 from 'web3';
import { signAndSendTxEvm } from '../shared/send_evm';
import { getContractAddress, getEvmEndpoint } from '../shared/utils';
import { globalLogger } from '../shared/utils/logger';

// For now only using Ethereum price feeds
export async function updateEvmPriceFeed(asset: Asset, price: string) {
  if (asset !== 'BTC' && asset !== 'ETH') {
    throw new Error(`Unsupported price feed asset: ${asset}`);
  }

  const evmClient = new Web3(getEvmEndpoint('Ethereum'));

  const priceFeedAddress = getContractAddress('Ethereum', `PRICE_FEED_${asset}`);

  // Not adding it in the contract interfaces folder because these are functions added in
  // our mock, while that interface is the real one declared by Chainlink.
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
  console.log('Price', price);
  const txData = priceFeedContract.methods.updatePrice(price).encodeABI();
  await signAndSendTxEvm(globalLogger, 'Ethereum', priceFeedAddress, '0', txData);
}
