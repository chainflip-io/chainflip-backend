import * as crypto from 'crypto';
import { EventEmitter } from 'events';
import { io, Socket } from 'socket.io-client';
import { promisify } from 'util';
import { QuoteRequest, MarketMakerResponse } from '../schemas';

const signAsync = promisify(crypto.sign);

type QuoteHandler = (
  quote: QuoteRequest,
) => Promise<Omit<MarketMakerResponse, 'id'>>;

/**
 * A reference implementation of a client that connects to the quoting service
 * and handles quote requests
 */
export default class QuotingClient extends EventEmitter {
  private socket!: Socket;

  private quoteHandler!: QuoteHandler;

  private privateKey: crypto.KeyObject;

  constructor(
    url: string,
    private readonly marketMakerId: string,
    privateKey: string,
  ) {
    super();
    this.privateKey = crypto.createPrivateKey({
      key: Buffer.from(privateKey),
      format: 'pem',
      type: 'pkcs8',
    });
    this.connect(url);
  }

  private async connect(url: string) {
    const timestamp = Date.now();
    this.socket = io(url, {
      auth: {
        timestamp,
        client_version: '1',
        market_maker_id: this.marketMakerId,
        signature: await this.getSignature(timestamp),
      },
    });

    this.socket.on('connect', () => {
      this.emit('connected');
    });

    this.socket.on('quote_request', async (quote: QuoteRequest) => {
      const response = await this.quoteHandler(quote);
      this.socket.emit('quote_response', { ...response, id: quote.id });
    });
  }

  private async getSignature(timestamp: number): Promise<string> {
    const buffer = await signAsync(
      null,
      Buffer.from(`${this.marketMakerId}${timestamp}`, 'utf8'),
      this.privateKey,
    );

    return buffer.toString('base64');
  }

  setQuoteRequestHandler(handler: QuoteHandler) {
    this.quoteHandler = handler;
  }

  close() {
    this.socket.close();
  }
}
