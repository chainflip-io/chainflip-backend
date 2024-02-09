-- CreateEnum
CREATE TYPE "public"."SwapFeeType" AS ENUM ('LIQUIDITY', 'NETWORK');

-- CreateTable
CREATE TABLE "public"."SwapFee" (
    "id" BIGSERIAL NOT NULL,
    "swapId" BIGINT NOT NULL,
    "type" "public"."SwapFeeType" NOT NULL,
    "asset" "public"."Asset" NOT NULL,
    "amount" DECIMAL(30,0) NOT NULL,

    CONSTRAINT "SwapFee_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "SwapFee_swapId_idx" ON "public"."SwapFee"("swapId");

-- AddForeignKey
ALTER TABLE "public"."SwapFee" ADD CONSTRAINT "SwapFee_swapId_fkey" FOREIGN KEY ("swapId") REFERENCES "public"."Swap"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
