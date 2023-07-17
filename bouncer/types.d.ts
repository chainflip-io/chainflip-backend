declare module 'bitcoin-core' {
  type Outpoint = { id: number; index: number };
  export default class Client extends RpcClient {
    constructor(opts: {
      agentOptions?: import('http').AgentOptions;
      allowDefaultWallet?: boolean;
      headers?: boolean;
      host?: string;
      network?: 'mainnet' | string;
      password?: string;
      port?: number;
      ssl?: boolean;
      timeout?: number;
      username?: string;
      version?: unknown;
      wallet?: string;
    });

    async command(...args: unknown[]): Promise<unknown>;

    async getTransactionByHash(hash: string, { extension = 'json' } = {}): Promise<unknown>;

    async getBlockByHash(
      hash: string,
      { summary = false, extension = 'json' } = {},
    ): Promise<unknown>;

    async getBlockHeadersByHash(
      hash: string,
      count: number,
      { extension = 'json' } = {},
    ): Promise<unknown>;

    async getBlockchainInformation(): Promise<unknown>;

    async getUnspentTransactionOutputs(
      outpoints: Outpoint | Outpoint[],
      { extension = 'json' } = {},
    ): Promise<unknown>;

    async getMemoryPoolContent(): Promise<unknown>;

    async getMemoryPoolInformation(): Promise<unknown>;

    abandonTransaction: (...args: unknown) => Promise<unknown>;

    abortRescan: (...args: unknown) => Promise<unknown>;

    addMultiSigAddress: (...args: unknown) => Promise<unknown>;

    addNode: (...args: unknown) => Promise<unknown>;

    addWitnessAddress: (...args: unknown) => Promise<unknown>;

    analyzePsbt: (...args: unknown) => Promise<unknown>;

    backupWallet: (...args: unknown) => Promise<unknown>;

    bumpFee: (...args: unknown) => Promise<unknown>;

    clearBanned: (...args: unknown) => Promise<unknown>;

    combinePsbt: (...args: unknown) => Promise<unknown>;

    combineRawTransaction: (...args: unknown) => Promise<unknown>;

    convertToPsbt: (...args: unknown) => Promise<unknown>;

    createMultiSig: (...args: unknown) => Promise<unknown>;

    createPsbt: (...args: unknown) => Promise<unknown>;

    createRawTransaction: (...args: unknown) => Promise<unknown>;

    createWallet: (...args: unknown) => Promise<unknown>;

    createWitnessAddress: (...args: unknown) => Promise<unknown>;

    decodePsbt: (...args: unknown) => Promise<unknown>;

    decodeRawTransaction: (...args: unknown) => Promise<unknown>;

    decodeScript: (...args: unknown) => Promise<unknown>;

    deriveAddresses: (...args: unknown) => Promise<unknown>;

    disconnectNode: (...args: unknown) => Promise<unknown>;

    dumpPrivKey: (...args: unknown) => Promise<unknown>;

    dumpTxOutset: (...args: unknown) => Promise<unknown>;

    dumpWallet: (...args: unknown) => Promise<unknown>;

    encryptWallet: (...args: unknown) => Promise<unknown>;

    estimateFee: (...args: unknown) => Promise<unknown>;

    estimatePriority: (...args: unknown) => Promise<unknown>;

    estimateSmartFee: (...args: unknown) => Promise<unknown>;

    estimateSmartPriority: (...args: unknown) => Promise<unknown>;

    finalizePsbt: (...args: unknown) => Promise<unknown>;

    fundRawTransaction: (...args: unknown) => Promise<unknown>;

    generate: (...args: unknown) => Promise<unknown>;

    generateToAddress: (...args: unknown) => Promise<unknown>;

    generateToDescriptor: (...args: unknown) => Promise<unknown>;

    getAccount: (...args: unknown) => Promise<unknown>;

    getAccountAddress: (...args: unknown) => Promise<unknown>;

    getAddedNodeInfo: (...args: unknown) => Promise<unknown>;

    getAddressInfo: (...args: unknown) => Promise<unknown>;

    getAddressesByAccount: (...args: unknown) => Promise<unknown>;

    getAddressesByLabel: (...args: unknown) => Promise<unknown>;

    getBalance: (...args: unknown) => Promise<unknown>;

    getBalances: (...args: unknown) => Promise<unknown>;

    getBestBlockHash: (...args: unknown) => Promise<unknown>;

    getBlock: (...args: unknown) => Promise<unknown>;

    getBlockCount: (...args: unknown) => Promise<unknown>;

    getBlockFilter: (...args: unknown) => Promise<unknown>;

    getBlockHash: (...args: unknown) => Promise<unknown>;

    getBlockHeader: (...args: unknown) => Promise<unknown>;

    getBlockStats: (...args: unknown) => Promise<unknown>;

    getBlockTemplate: (...args: unknown) => Promise<unknown>;

    getBlockchainInfo: (...args: unknown) => Promise<unknown>;

    getChainTips: (...args: unknown) => Promise<unknown>;

    getChainTxStats: (...args: unknown) => Promise<unknown>;

    getConnectionCount: (...args: unknown) => Promise<unknown>;

    getDeploymentInfo: (...args: unknown) => Promise<unknown>;

    getDescriptorInfo: (...args: unknown) => Promise<unknown>;

    getDifficulty: (...args: unknown) => Promise<unknown>;

    getGenerate: (...args: unknown) => Promise<unknown>;

    getHashesPerSec: (...args: unknown) => Promise<unknown>;

    getIndexInfo: (...args: unknown) => Promise<unknown>;

    getInfo: (...args: unknown) => Promise<unknown>;

    getMemoryInfo: (...args: unknown) => Promise<unknown>;

    getMempoolAncestors: (...args: unknown) => Promise<unknown>;

    getMempoolDescendants: (...args: unknown) => Promise<unknown>;

    getMempoolEntry: (...args: unknown) => Promise<unknown>;

    getMempoolInfo: (...args: unknown) => Promise<unknown>;

    getMiningInfo: (...args: unknown) => Promise<unknown>;

    getNetTotals: (...args: unknown) => Promise<unknown>;

    getNetworkHashPs: (...args: unknown) => Promise<unknown>;

    getNetworkInfo: (...args: unknown) => Promise<unknown>;

    getNewAddress: (...args: unknown) => Promise<string>;

    getNodeAddresses: (...args: unknown) => Promise<unknown>;

    getPeerInfo: (...args: unknown) => Promise<unknown>;

    getRawChangeAddress: (...args: unknown) => Promise<unknown>;

    getRawMempool: (...args: unknown) => Promise<unknown>;

    getRawTransaction: (...args: unknown) => Promise<unknown>;

    getReceivedByAccount: (...args: unknown) => Promise<unknown>;

    getReceivedByAddress: (...args: unknown) => Promise<unknown>;

    getReceivedByLabel: (...args: unknown) => Promise<unknown>;

    getRpcInfo: (...args: unknown) => Promise<unknown>;

    getTransaction: (...args: unknown) => Promise<{ confirmations: number }>;

    getTxOut: (...args: unknown) => Promise<unknown>;

    getTxOutProof: (...args: unknown) => Promise<unknown>;

    getTxOutSetInfo: (...args: unknown) => Promise<unknown>;

    getUnconfirmedBalance: (...args: unknown) => Promise<unknown>;

    getWalletInfo: (...args: unknown) => Promise<unknown>;

    getWork: (...args: unknown) => Promise<unknown>;

    getZmqNotifications: (...args: unknown) => Promise<unknown>;

    help: (...args: unknown) => Promise<unknown>;

    importAddress: (...args: unknown) => Promise<unknown>;

    importMulti: (...args: unknown) => Promise<unknown>;

    importPrivKey: (...args: unknown) => Promise<unknown>;

    importPrunedFunds: (...args: unknown) => Promise<unknown>;

    importPubKey: (...args: unknown) => Promise<unknown>;

    importWallet: (...args: unknown) => Promise<unknown>;

    joinPsbts: (...args: unknown) => Promise<unknown>;

    keypoolRefill: (...args: unknown) => Promise<unknown>;

    listAccounts: (...args: unknown) => Promise<unknown>;

    listAddressGroupings: (...args: unknown) => Promise<unknown>;

    listBanned: (...args: unknown) => Promise<unknown>;

    listLabels: (...args: unknown) => Promise<unknown>;

    listLockUnspent: (...args: unknown) => Promise<unknown>;

    listReceivedByAccount: (...args: unknown) => Promise<unknown>;

    listReceivedByAddress: (...args: unknown) => Promise<{ amount: number }[]>;

    listReceivedByLabel: (...args: unknown) => Promise<unknown>;

    listSinceBlock: (...args: unknown) => Promise<unknown>;

    listTransactions: (...args: unknown) => Promise<unknown>;

    listUnspent: (...args: unknown) => Promise<{ amount: number; txid: string; vout: string }[]>;

    listWalletDir: (...args: unknown) => Promise<unknown>;

    listWallets: (...args: unknown) => Promise<unknown>;

    loadWallet: (...args: unknown) => Promise<unknown>;

    lockUnspent: (...args: unknown) => Promise<unknown>;

    logging: (...args: unknown) => Promise<unknown>;

    move: (...args: unknown) => Promise<unknown>;

    ping: (...args: unknown) => Promise<unknown>;

    preciousBlock: (...args: unknown) => Promise<unknown>;

    prioritiseTransaction: (...args: unknown) => Promise<unknown>;

    pruneBlockchain: (...args: unknown) => Promise<unknown>;

    removePrunedFunds: (...args: unknown) => Promise<unknown>;

    rescanBlockchain: (...args: unknown) => Promise<unknown>;

    saveMempool: (...args: unknown) => Promise<unknown>;

    scantxoutset: (...args: unknown) => Promise<unknown>;

    sendFrom: (...args: unknown) => Promise<unknown>;

    sendMany: (...args: unknown) => Promise<unknown>;

    sendRawTransaction: (...args: unknown) => Promise<unknown>;

    sendToAddress: (...args: unknown) => Promise<unknown>;

    setAccount: (...args: unknown) => Promise<unknown>;

    setBan: (...args: unknown) => Promise<unknown>;

    setGenerate: (...args: unknown) => Promise<unknown>;

    setHdSeed: (...args: unknown) => Promise<unknown>;

    setLabel: (...args: unknown) => Promise<unknown>;

    setNetworkActive: (...args: unknown) => Promise<unknown>;

    setTxFee: (...args: unknown) => Promise<unknown>;

    setWalletFlag: (...args: unknown) => Promise<unknown>;

    signMessage: (...args: unknown) => Promise<unknown>;

    signMessageWithPrivKey: (...args: unknown) => Promise<unknown>;

    signRawTransaction: (...args: unknown) => Promise<unknown>;

    signRawTransactionWithKey: (...args: unknown) => Promise<unknown>;

    signRawTransactionWithWallet: (...args: unknown) => Promise<{ hex: string }>;

    stop: (...args: unknown) => Promise<unknown>;

    submitBlock: (...args: unknown) => Promise<unknown>;

    testMempoolAccept: (...args: unknown) => Promise<unknown>;

    unloadWallet: (...args: unknown) => Promise<unknown>;

    upTime: (...args: unknown) => Promise<unknown>;

    utxoUpdatePsbt: (...args: unknown) => Promise<unknown>;

    validateAddress: (...args: unknown) => Promise<unknown>;

    verifyChain: (...args: unknown) => Promise<unknown>;

    verifyMessage: (...args: unknown) => Promise<unknown>;

    verifyTxOutProof: (...args: unknown) => Promise<unknown>;

    walletCreateFundedPsbt: (...args: unknown) => Promise<unknown>;

    walletLock: (...args: unknown) => Promise<unknown>;

    walletPassphrase: (...args: unknown) => Promise<unknown>;

    walletPassphraseChange: (...args: unknown) => Promise<unknown>;

    walletProcessPsbt: (...args: unknown) => Promise<unknown>;
  }
}
