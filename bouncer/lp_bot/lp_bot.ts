import { webSocket, WebSocketSubject } from 'rxjs/webSocket';
import { Observable, Subject, from } from 'rxjs';
import { filter, map, mergeMap } from 'rxjs/operators';
import WebSocket from 'ws';
import { lpApiRpc } from '../shared/json_rpc';
import {
  chainFromAsset,
  createStateChainKeypair,
  getContractAddress,
  toAsset,
} from '../shared/utils';
import { globalLogger as logger } from '../shared/utils/logger';
import { sendErc20 } from '../shared/send_erc20';
import {
  Order,
  Side,
  OrderStatus,
  OrderType,
  Swap,
  TradeDecision,
  OutOfLiquidityEvent,
} from './utils';
import { sendBtc } from '../shared/send_btc';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
(global as any).WebSocket = WebSocket;

const outOfLiquiditySubject = new Subject<OutOfLiquidityEvent>();
class LPBotState {
  ORDER_BOOK: Map<number, Order>;

  SWAPS: Map<number, Swap>;

  LP_ACCOUNT: string;

  CHAIN: string;

  ASSET: string;

  constructor(chain: string, asset: string) {
    this.ORDER_BOOK = new Map<number, Order>();
    this.SWAPS = new Map<number, Swap>();
    this.LP_ACCOUNT = createStateChainKeypair('//LP_API').address;
    this.CHAIN = chain;
    this.ASSET = asset;
  }
}

/**
 * Determines if a swap should be executed.
 *
 * @param swap - The swap to evaluate.
 * @returns The trade decision.
 */
const tradingStrategy = (swap: Swap): TradeDecision => ({
  shouldTrade: true,
  side: swap.side === Side.Sell ? Side.Buy : Side.Sell, // If someone sells we buy and vice versa
  asset: swap.baseAsset.asset,
  amount: swap.amount,
  price: 0, // We always use the current pool price
});

/**
 * Manages limit orders.
 *
 * @param decision - The trade decision.
 * @returns The order ID.
 */
const manageLimitOrders = async (state: LPBotState, decision: TradeDecision) => {
  const currentOpenOrderForAsset = Array.from(state.ORDER_BOOK.values()).find(
    (order) =>
      order.asset === decision.asset &&
      order.orderType === OrderType.Limit &&
      order.amount >= decision.amount,
  );

  if (currentOpenOrderForAsset) {
    logger.info(`Found existing order for asset: ${decision.asset}`);
    return currentOpenOrderForAsset.orderId;
  }

  const orderId = Math.floor(Math.random() * 10000) + 1;

  try {
    logger.info(
      `Setting limit order for asset: ${decision.asset}, amount: ${decision.amount}, side: ${decision.side}, orderId: ${orderId}`,
    );
    await lpApiRpc(logger, 'lp_set_limit_order', [
      { chain: state.CHAIN, asset: decision.asset },
      { chain: 'Ethereum', asset: 'USDC' },
      decision.side,
      orderId,
      0,
      decision.amount,
    ]);
    state.ORDER_BOOK.set(
      orderId,
      new Order(
        orderId,
        OrderStatus.Accepted,
        OrderType.Limit,
        decision.asset,
        decision.side,
        decision.amount,
        decision.price,
      ),
    );
    return orderId;
  } catch (error) {
    logger.error(`Failed to execute order: ${error}`);
    outOfLiquiditySubject.next({
      side: decision.side,
      asset: decision.asset,
      amount: decision.amount,
    } as OutOfLiquidityEvent);
    return null;
  }
};

/**
 * Creates a swap stream.
 *
 * @param wsConnection - The WebSocket connection.
 * @returns The swap stream.
 */
const createSwapStream = (
  state: LPBotState,
  wsConnection: WebSocketSubject<unknown>,
): Observable<Swap> =>
  wsConnection.pipe(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    filter((msg: any) => msg.method === 'cf_subscribe_scheduled_swaps'),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    map((msg: any) => msg.params.result.swaps),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    mergeMap((swaps: any[]) => from(swaps)),
    map(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (swap: any): Swap => ({
        swapId: swap.swap_id,
        baseAsset: swap.base_asset,
        quoteAsset: swap.quote_asset,
        side: swap.side,
        amount: swap.amount,
      }),
    ),
    filter((swap: Swap) => {
      if (state.SWAPS.has(swap.swapId)) {
        return false;
      }
      state.SWAPS.set(swap.swapId, swap);
      return true;
    }),
  );

/**
 * Creates a trade decision stream.
 *
 * @param swaps$ - The swap stream.
 * @returns The trade decision stream.
 */
const createTradeDecisionStream = (swaps$: Observable<Swap>): Observable<TradeDecision> =>
  swaps$.pipe(
    map(tradingStrategy),
    filter((decision) => decision.shouldTrade),
  );

/**
 * Creates an order stream.
 *
 * @param decisions$ - The trade decision stream.
 * @returns The order stream.
 */
const createOrderStream = (
  state: LPBotState,
  decisions$: Observable<TradeDecision>,
): Observable<number | null> =>
  decisions$.pipe(
    map((decision) => manageLimitOrders(state, decision)),
    mergeMap((orderId) => from(orderId)),
  );

/**
 * Creates an order fill stream.
 *
 * @param wsConnection - The WebSocket connection.
 * @returns The order fill stream.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const createOrderFillStream = (wsConnection: WebSocketSubject<unknown>): Observable<any> =>
  wsConnection.pipe(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    filter((msg: any) => msg.method === 'lp_subscribe_order_fills'),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    map((msg: any) => msg.params.result.fills),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    mergeMap((fills: any[]) => from(fills)),
  );

/**
 * Deposits liquidity.
 *
 * Note: Only Ethereum is supported for now.
 *
 * @param rpcAsset - The RPC asset.
 * @param amount - The amount of liquidity to deposit.
 */
const depositLiquidity = async (rpcAsset: string, amount: string) => {
  logger.info(`Try to deposit liquidity for asset: ${rpcAsset}, amount: ${amount}`);
  const asset = toAsset(rpcAsset);
  const chain = chainFromAsset(asset);
  let liquidityDepositAddress;
  switch (chain) {
    case 'Ethereum':
      liquidityDepositAddress = await lpApiRpc(logger, 'lp_liquidity_deposit', [
        { chain, asset: rpcAsset },
        'InBlock',
      ]);
      await sendErc20(
        logger,
        chain,
        liquidityDepositAddress.tx_details.response,
        getContractAddress(chain, asset),
        amount,
      );
      logger.info(`Liquidity sent: ${amount} ${asset}!`);
      break;
    case 'Bitcoin':
      liquidityDepositAddress = await lpApiRpc(logger, 'lp_liquidity_deposit', [
        { chain, asset: rpcAsset },
        'InBlock',
      ]);
      await sendBtc(
        logger,
        chain,
        liquidityDepositAddress.tx_details.response,
        parseInt(amount, 16),
      );
      logger.info(`Liquidity sent: ${amount} ${asset}!`);
      break;
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
};

/**
 * Initializes the liquidity provider bot.
 *
 * @returns The state chain and liquidity provider WebSocket connections.
 */
const initializeLiquidityProviderBot = (chain: string, asset: string) => {
  logger.info(`Initializing liquidity provider bot for chain: ${chain}, asset: ${asset} 🤖.`);

  const state = new LPBotState(chain, asset);

  logger.info(`LP Account: ${state.LP_ACCOUNT}`);

  const stateChainWsConnection = webSocket('ws://127.0.0.1:9944');
  const lpWsConnection = webSocket('ws://127.0.0.1:10589');

  // Create our reactive pipeline
  const swaps$ = createSwapStream(state, stateChainWsConnection);
  const tradeDecisions$ = createTradeDecisionStream(swaps$);
  const orders$ = createOrderStream(state, tradeDecisions$);
  const orderFills$ = createOrderFillStream(lpWsConnection);
  const outOfLiquidity$ = outOfLiquiditySubject.asObservable();

  // Subscribe to order fills stream
  orderFills$.subscribe({
    next: (fill) => {
      if (fill.limit_order) {
        if (fill.limit_order.lp === state.LP_ACCOUNT) {
          logger.info(`We won a swap 🕺 !`);
          const idAsNumber = parseInt(fill.limit_order.id);
          const order = state.ORDER_BOOK.get(idAsNumber)!;
          order.status = OrderStatus.Filled;
          state.ORDER_BOOK.set(idAsNumber, order);
        }
      }
    },
    error: (err) => logger.error('Error in order fills stream:', err),
    complete: () => logger.info('Order fills stream completed'),
  });

  outOfLiquidity$.subscribe({
    next: () => {
      logger.info('Out of liquidity 💸!');
      // let decAmount = parseInt(event.amount.toString(), 16);
      // let decimals = assetDecimals(toAsset(event.asset));
      // logger.info(`Depositing liquidity for asset: ${event.asset}, hexAmount: ${event.amount} 🤗 dexAmount: ${parseInt(event.amount.toString(), 16)} 🤗`);
      // depositLiquidity(event.asset, event.amount);
    },
  });

  // Subscribe to the final stream to start the flow
  orders$.subscribe({
    next: (orderId) => {
      if (orderId) {
        logger.info(`Submitted order: ${orderId} successfully ✅.`);
      }
    },
    error: (err) => logger.error('Error in order stream:', err),
    complete: () => logger.info('Order stream completed'),
  });

  swaps$.subscribe({
    next: (swap) => {
      logger.info(
        `Received new swap Id: ${swap.swapId}, amount: ${swap.amount}, side: ${swap.side}, baseAsset: ${swap.baseAsset}, quoteAsset: ${swap.quoteAsset} 🔄.`,
      );
    },
    error: (err) => logger.error('Error in swap stream:', err),
    complete: () => logger.info('Swap stream completed'),
  });

  stateChainWsConnection.next({
    id: 1,
    jsonrpc: '2.0',
    method: 'cf_subscribe_scheduled_swaps',
    params: {
      base_asset: { chain: state.CHAIN, asset: state.ASSET },
      quote_asset: { chain: 'Ethereum', asset: 'USDC' },
    },
  });

  lpWsConnection.next({
    id: 1,
    jsonrpc: '2.0',
    method: 'lp_subscribe_order_fills',
    params: [],
  });

  return [stateChainWsConnection, lpWsConnection];
};

export { initializeLiquidityProviderBot, depositLiquidity };
