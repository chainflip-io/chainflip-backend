import { BN } from '@polkadot/util';
import { Asset, Chain } from '@chainflip/cli';
import Web3 from 'web3';
import { signAndSendTxEvm } from '../shared/send_evm';
import { amountToFineAmount, getContractAddress, getEvmEndpoint } from '../shared/utils';
import { Logger } from '../shared/utils/logger';
import { PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js';
import { signAndSendTxSol } from './send_sol';

// All price feeds are using 8 decimals
const PRICE_FEED_DECIMALS = 8;

export async function updatePriceFeed(logger: Logger, chain: Chain, asset: Asset, price: string) {
  if (asset !== 'BTC' && asset !== 'ETH') {
    throw new Error(`Unsupported price feed asset: ${asset}`);
  }

  // For now only using Ethereum price feeds
  switch (chain) {
    case 'Ethereum':
      await updateEvmPriceFeed(logger, 'Ethereum', asset, price);
    case 'Solana':
      return updateSolanaPriceFeed(logger, asset, price);
    default:
      throw new Error(`Unsupported chain for price feed update: ${chain}`);
  }
}

async function updateSolanaPriceFeed(logger: Logger, asset: Asset, price: string) {
  const finePrice = amountToFineAmount(price, PRICE_FEED_DECIMALS);
  const priceFeedMockAddress = getContractAddress('Solana', `PRICE_FEED_MOCK`);
  const priceFeedAddress = new PublicKey(getContractAddress('Solana', `PRICE_FEED_${asset}`));

  const updatePriceDiscriminator = Buffer.from([61, 34, 117, 155, 75, 34, 123, 208]);

  const priceBN = new BN(finePrice);
  const priceBuffer = priceBN.toBuffer('le', 16);

  let tx = new Transaction().add(
    new TransactionInstruction({
      data: Buffer.concat([Buffer.from(updatePriceDiscriminator), priceBuffer]),
      keys: [{ pubkey: priceFeedAddress, isSigner: false, isWritable: true }],
      programId: new PublicKey(priceFeedMockAddress),
    }),
  );
  await signAndSendTxSol(logger, tx);
}

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
