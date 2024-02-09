-- CreateEnum
CREATE TYPE "Network" AS ENUM ('Polkadot', 'Ethereum');

-- CreateTable
CREATE TABLE "SwapIntent" (
    "id" BIGSERIAL NOT NULL,
    "ingressAsset" TEXT NOT NULL,
    "ingressAddress" TEXT NOT NULL,
    "egressAsset" TEXT NOT NULL,
    "egressAddress" TEXT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "SwapIntent_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Swap" (
    "id" BIGSERIAL NOT NULL,
    "nativeId" BIGINT NOT NULL,
    "ingressAmount" DECIMAL(30,0) NOT NULL,
    "ingressReceivedAt" TIMESTAMP(3) NOT NULL,
    "swapExecutedAt" TIMESTAMP(3),
    "egressCompleteAt" TIMESTAMP(3),
    "swapIntentId" BIGINT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "Swap_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Egress" (
    "id" BIGSERIAL NOT NULL,
    "nativeId" BIGINT NOT NULL,
    "network" "Network" NOT NULL,
    "amount" DECIMAL(30,0) NOT NULL,
    "timestamp" TIMESTAMP(3) NOT NULL,
    "swapId" BIGINT,
    "broadcastId" BIGINT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "Egress_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "Broadcast" (
    "id" BIGSERIAL NOT NULL,
    "nativeId" BIGINT NOT NULL,
    "network" "Network" NOT NULL,

    CONSTRAINT "Broadcast_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "SwapIntent_ingressAddress_key" ON "SwapIntent"("ingressAddress");

-- CreateIndex
CREATE UNIQUE INDEX "Swap_nativeId_key" ON "Swap"("nativeId");

-- CreateIndex
CREATE UNIQUE INDEX "Egress_swapId_key" ON "Egress"("swapId");

-- CreateIndex
CREATE UNIQUE INDEX "Egress_nativeId_network_key" ON "Egress"("nativeId", "network");

-- CreateIndex
CREATE UNIQUE INDEX "Broadcast_nativeId_network_key" ON "Broadcast"("nativeId", "network");

-- AddForeignKey
ALTER TABLE "Swap" ADD CONSTRAINT "Swap_swapIntentId_fkey" FOREIGN KEY ("swapIntentId") REFERENCES "SwapIntent"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Egress" ADD CONSTRAINT "Egress_swapId_fkey" FOREIGN KEY ("swapId") REFERENCES "Swap"("id") ON DELETE SET NULL ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "Egress" ADD CONSTRAINT "Egress_broadcastId_fkey" FOREIGN KEY ("broadcastId") REFERENCES "Broadcast"("id") ON DELETE SET NULL ON UPDATE CASCADE;
