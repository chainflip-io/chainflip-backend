import axios from 'axios';
import { TestContext } from 'shared/utils/test_context';
import { globalLogger } from 'shared/utils/logger';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';
import { testSol, testSolVaultSwap } from 'tests/broker_level_screening/sol';
import {
  testEvm,
  testEvmVaultSwap,
  testEvmLiquidityDeposit,
} from 'tests/broker_level_screening/evm';
import { testBitcoin, testBitcoinVaultSwap } from 'tests/broker_level_screening/bitcoin';
import { testTron, testTronVaultSwap } from 'tests/broker_level_screening/tron';

/**
 * Submit a post request to the deposit-monitor, with error handling.
 * @param portAndRoute Where we want to submit the request to.
 * @param body The request body, is serialized as JSON.
 */
async function postToDepositMonitor(portAndRoute: string, body: string | object) {
  return axios
    .post('http://127.0.0.1' + portAndRoute, JSON.stringify(body), {
      headers: {
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
      timeout: 20000,
    })
    .then((res) => res.data)
    .catch((error) => {
      let message;
      if (error.response) {
        message = `${error.response.data} (${error.response.status})`;
      } else {
        message = error;
      }
      throw new Error(`Request to deposit monitor (${portAndRoute}) failed: ${message}`);
    });
}

/**
 * Typescript representation of the allowed parameters to `setMockmode`. The JSON encoding of these
 * is what the deposit-monitor expects.
 */
type Mockmode =
  | 'Manual'
  | { Deterministic: { score: number; incomplete_probability: number } }
  | { Random: { min_score: number; max_score: number; incomplete_probability: number } };

/**
 * Set the mockmode of the deposit monitor, controlling how it analyses incoming transactions.
 *
 * @param mode Object describing the mockmode we want to set the deposit-monitor to,
 */
async function setMockmode(mode: Mockmode) {
  return postToDepositMonitor(':6070/mockmode', mode);
}

/**
 * Call the deposit-monitor to set risk score of given transaction in mock analysis provider.
 *
 * @param txid Hash of the transaction we want to report.
 * @param score Risk score for this transaction. Can be in range [0.0, 10.0].
 */
async function setTxRiskScore(txid: string, score: number) {
  await postToDepositMonitor(':6070/riskscore', [
    txid,
    {
      risk_score: { Score: score },
      unknown_contribution_percentage: 0.0,
    },
  ]);
}

/**
 * Checks that the deposit monitor has started up successfully and is healthy.
 */
async function ensureHealth() {
  const response = await postToDepositMonitor(':6060/health', {});
  globalLogger.info(`DM health response is: ${JSON.stringify(response)}`);
  if (response.starting === true || response.all_processors === false) {
    throw new Error(
      `Deposit monitor is running, but not healthy. It's response was: ${JSON.stringify(response)}`,
    );
  }
}

// Sets the ingress_egress broker whitelist to the given `broker`.
async function setWhitelistedBroker<A = []>(cf: ChainflipIO<A>, brokerAddress: Uint8Array) {
  const BTC_WHITELIST_PREFIX = '3ed3ce16dbc61ca64eaac5a96e809a8f6b8fb02fc586c9dab2385ea1690a7db6';
  const ETH_WHITELIST_PREFIX = '4fc967eb3d0785df0389312c2ebd853e6b8fb02fc586c9dab2385ea1690a7db6';
  const ARB_WHITELIST_PREFIX = '3d3491b8c14ff78a5176bc3b6ebe516f6b8fb02fc586c9dab2385ea1690a7db6';
  const SOL_WHITELIST_PREFIX = '8595efe3a571f61007e89f4416b858b16b8fb02fc586c9dab2385ea1690a7db6';
  const TRON_WHITELIST_PREFIX = '65fbb72d24f6d3ade3baaf42fd5075756b8fb02fc586c9dab2385ea1690a7db6';

  const decodeHexStringToByteArray = (hex: string) => {
    let hexString = hex;
    const result = [];
    while (hexString.length >= 2) {
      result.push(parseInt(hexString.substring(0, 2), 16));
      hexString = hexString.substring(2, hexString.length);
    }
    return result;
  };

  await cf.all(
    [
      BTC_WHITELIST_PREFIX,
      ETH_WHITELIST_PREFIX,
      ARB_WHITELIST_PREFIX,
      SOL_WHITELIST_PREFIX,
      TRON_WHITELIST_PREFIX,
    ].map(
      (prefix) => (subcf: ChainflipIO<A>) =>
        subcf.submitGovernance({
          extrinsic: (api) =>
            api.tx.governance.callAsSudo(
              api.tx.system.setStorage([
                [
                  decodeHexStringToByteArray(prefix).concat(Array.from(brokerAddress)),
                  // Empty, we just need to insert the key.
                  '',
                ],
              ]),
            ),
        }),
    ),
  );
}

export async function doTestSwapDeposits<A = []>(
  cf: ChainflipIO<A>,
  testBoostedDeposits: boolean = false,
) {
  // NOTE: We currently don't test the following assets:
  // - Flip: we don't test Flip rejections because they are currently disabled in the
  //         deposit monitor, since Elliptic doesn't provide Flip analysis.
  // - ArbEth: we don't test ArbEth rejections since on localnet the safety margin for ArbEth
  //           is too short for the DM, the rejections fail more often than not due
  //           to being too late.
  //           Most of the functionality is covered by testing `Eth` and `ArbUsdc`.
  //           An alternative would be to increase the ArbEth safety margin on localnet.
  // - ArbUsdc: we also don't test ArbUsdc rejections, they have caused tests to become flaky
  //            as well (PRO-2488).
  // - Btc VaultSwaps: For bitcoin, due to ancestor screening, we have to make sure to use
  //                   a dedicated "tainted" wallet. Since it's somewhat difficult to inject
  //                   a different wallet into the `sendVaultSwap` flow, we disable the test for now.

  // test rejection of swaps by the responsible broker
  await cf.all([
    (subcf) => testTron(subcf, 'Trx', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testTron(subcf, 'TrxUsdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testSol(subcf, 'Sol', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testSol(subcf, 'SolUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testSol(subcf, 'SolUsdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvm(subcf, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvm(subcf, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvm(subcf, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvm(subcf, 'Wbtc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) =>
      testBitcoin(subcf.withChildLogger('BrokerLevelScreening_testBitcoin'), false, async (txId) =>
        setTxRiskScore(txId, 9.0),
      ),
    ...(testBoostedDeposits
      ? [
          (subcf: ChainflipIO<A>) =>
            testBitcoin(
              subcf.withChildLogger('BrokerLevelScreening_testBitcoin_boost'),
              true,
              async (txId) => setTxRiskScore(txId, 9.0),
            ),
        ]
      : []),
  ]);
}

export async function doTestLpDeposits<A = []>(parentCf: ChainflipIO<A>) {
  const cf = parentCf.with({ account: fullAccountFromUri('//LP_1', 'LP') });

  await cf.all([
    // --- LP deposits ---
    (subcf) => testEvmLiquidityDeposit(subcf, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmLiquidityDeposit(subcf, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmLiquidityDeposit(subcf, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmLiquidityDeposit(subcf, 'Wbtc', async (txId) => setTxRiskScore(txId, 9.0)),
  ]);
}

export async function doTestVaultSwaps<A = []>(cf: ChainflipIO<A>) {
  await cf.with({ account: fullAccountFromUri('//BROKER_1', 'Broker') }).all([
    // --- vault swaps ---
    (subcf) => testBitcoinVaultSwap(subcf, async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmVaultSwap(subcf, 'Eth', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmVaultSwap(subcf, 'Usdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmVaultSwap(subcf, 'Usdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testEvmVaultSwap(subcf, 'Wbtc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testSolVaultSwap(subcf, 'Sol', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testSolVaultSwap(subcf, 'SolUsdc', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testSolVaultSwap(subcf, 'SolUsdt', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testTronVaultSwap(subcf, 'Trx', async (txId) => setTxRiskScore(txId, 9.0)),
    (subcf) => testTronVaultSwap(subcf, 'TrxUsdt', async (txId) => setTxRiskScore(txId, 9.0)),
  ]);
}

export async function testBrokerLevelScreening(
  testContext: TestContext,
  testBoostedDeposits: boolean = false,
) {
  const cf = await newChainflipIO(testContext.logger, []);

  await ensureHealth();
  const previousMockmode = (await setMockmode('Manual')).previous;

  // test rejection of LP deposits and vault swaps:
  //  - this requires the rejecting broker to be whitelisted
  //  - for bitcoin vault swaps a private channel has to be opened
  cf.debug('Whitelisting the broker api broker');
  await setWhitelistedBroker(cf, fullAccountFromUri('//BROKER_API', 'Broker').keypair.addressRaw);

  cf.debug('Launching broker level screening tests...');
  await cf.all([
    (subcf) => doTestLpDeposits(subcf),
    (subcf) => doTestVaultSwaps(subcf),
    (subcf) => doTestSwapDeposits(subcf, testBoostedDeposits),
  ]);

  await setMockmode(previousMockmode);
}
