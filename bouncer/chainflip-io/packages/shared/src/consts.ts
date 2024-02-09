import { ChainflipNetwork, ChainflipNetworks } from './enums';

// TODO: read this value via rpc once there is an appropriate rpc method
const POOLS_NETWORK_FEE_HUNDREDTH_PIPS: Partial<
  Record<ChainflipNetwork, number>
> = {
  [ChainflipNetworks.backspin]: 1000,
  [ChainflipNetworks.sisyphos]: 1000,
  [ChainflipNetworks.perseverance]: 1000,
  [ChainflipNetworks.mainnet]: 1000,
};
export const getPoolsNetworkFeeHundredthPips = (network: ChainflipNetwork) =>
  POOLS_NETWORK_FEE_HUNDREDTH_PIPS[network] ?? 0;

// https://developers.circle.com/developer/docs/usdc-on-testnet#usdc-on-ethereum-goerli
const GOERLI_USDC_CONTRACT_ADDRESS =
  '0x07865c6E87B9F70255377e024ace6630C1Eaa37F';

export const ADDRESSES = {
  [ChainflipNetworks.backspin]: {
    FLIP_CONTRACT_ADDRESS: '0x10C6E9530F1C1AF873a391030a1D9E8ed0630D26',
    USDC_CONTRACT_ADDRESS: '0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0',
    VAULT_CONTRACT_ADDRESS: '0xB7A5bd0345EF1Cc5E66bf61BdeC17D2461fBd968',
    STATE_CHAIN_GATEWAY_ADDRESS: '0xeEBe00Ac0756308ac4AaBfD76c05c4F3088B8883',
  },
  [ChainflipNetworks.sisyphos]: {
    FLIP_CONTRACT_ADDRESS: '0x2BbB561C6eaB74f358cA9e8a961E3A20CAE3D100',
    USDC_CONTRACT_ADDRESS: GOERLI_USDC_CONTRACT_ADDRESS,
    VAULT_CONTRACT_ADDRESS: '0xC17CCec5015081EB2DF26d20A9e02c5484C1d641',
    STATE_CHAIN_GATEWAY_ADDRESS: '0xE8bE4B7F8a38C1913387c9C20B94402bc3Db9F70',
  },
  [ChainflipNetworks.perseverance]: {
    FLIP_CONTRACT_ADDRESS: '0x0485D65da68b2A6b48C3fA28D7CCAce196798B94',
    USDC_CONTRACT_ADDRESS: GOERLI_USDC_CONTRACT_ADDRESS,
    VAULT_CONTRACT_ADDRESS: '0x40caFF3f3B6706Da904a7895e0fC7F7922437e9B',
    STATE_CHAIN_GATEWAY_ADDRESS: '0x38AA40B7b5a70d738baBf6699a45DacdDBBEB3fc',
  },
  [ChainflipNetworks.mainnet]: {
    FLIP_CONTRACT_ADDRESS: '0x826180541412D574cf1336d22c0C0a287822678A',
    USDC_CONTRACT_ADDRESS: '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48',
    VAULT_CONTRACT_ADDRESS: '0xF5e10380213880111522dd0efD3dbb45b9f62Bcc',
    STATE_CHAIN_GATEWAY_ADDRESS: '0x6995Ab7c4D7F4B03f467Cf4c8E920427d9621DBd',
  },
} as const;
