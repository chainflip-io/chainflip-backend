-- CreateTable
CREATE TABLE "public"."FailedSwap" (
    "id" SERIAL NOT NULL,
    "destinationAddress" TEXT NOT NULL,
    "destinationChain" "public"."Chain" NOT NULL,
    "depositAmount" DECIMAL(30,0) NOT NULL,
    "sourceChain" "public"."Chain" NOT NULL,
    "swapDepositChannelId" BIGINT,
    "txHash" TEXT,

    CONSTRAINT "FailedSwap_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "FailedSwap_swapDepositChannelId_idx" ON "public"."FailedSwap"("swapDepositChannelId");

-- AddForeignKey
ALTER TABLE "public"."FailedSwap" ADD CONSTRAINT "FailedSwap_swapDepositChannelId_fkey" FOREIGN KEY ("swapDepositChannelId") REFERENCES "public"."SwapDepositChannel"("id") ON DELETE SET NULL ON UPDATE CASCADE;
