import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import {
  observeEvent,
  getAddress,
  getChainflipApi,
  encodeDotAddressForContract,
  assetToChain,
  handleSubstrateError,
  encodeBtcAddressForContract,
  lpMutex,
} from '../shared/utils';
import { send } from '../shared/send';
import { Asset } from '@chainflip-io/cli/.';

const chain = new Map<Asset, string>([
  ['DOT', 'dot'],
  ['ETH', 'eth'],
  ['BTC', 'btc'],
  ['USDC', 'eth'],
  ['FLIP', 'eth'],
]);

export async function provideLiquidity(ccy: Asset, amount: number) {
  const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  const lp_uri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lp_uri);

  // If no emergency withdrawal address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.emergencyWithdrawalAddress(
        lp.address,
        assetToChain(ccy),
      )
    ).toJSON() === null
  ) {
    let emergencyAddress = await getAddress(ccy, 'LP_1');
    emergencyAddress =
      ccy === 'DOT' ? encodeDotAddressForContract(emergencyAddress) : emergencyAddress;

    console.log('Registering Emergency Withdrawal Address: ' + emergencyAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerEmergencyWithdrawalAddress({ [chain.get(ccy)!]: emergencyAddress })
        .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
    });
  }

  console.log('Requesting ' + ccy + ' deposit address');
  let eventHandle =
    chain.get(ccy) === 'eth'
      ? observeEvent('ethereumIngressEgress:StartWitnessing', chainflip, (data) => {
          return data[1].toUpperCase() === ccy;
        })
      : observeEvent('liquidityProvider:LiquidityDepositAddressReady', chainflip, (data) => {
          return data[1][chain.get(ccy)!] != undefined;
        });
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy.toLowerCase())
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  let ingress_address =
    chain.get(ccy) === 'eth'
      ? (await eventHandle).depositAddress.toJSON()
      : (await eventHandle).depositAddress.toJSON()[chain.get(ccy)!];
  if (ccy == 'BTC') {
    ingress_address = encodeBtcAddressForContract(ingress_address);
  }
  console.log('Received ' + ccy + ' address: ' + ingress_address);
  console.log('Sending ' + amount + ' ' + ccy + ' to ' + ingress_address);
  eventHandle = observeEvent('liquidityProvider:AccountCredited', chainflip, (data) => {
    return data[1].toUpperCase() == ccy;
  });
  send(ccy, ingress_address, String(amount));
  await eventHandle;
}
