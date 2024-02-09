/*
  Warnings:

  - You are about to drop the `SwapIntentBlock` table. If the table is not empty, all the data it contains will be lost.

*/
-- DropTable
DROP TABLE "public"."SwapIntentBlock";

-- CreateTable
CREATE TABLE "public"."SwapDepositChannelBlock" (
    "id" BIGSERIAL NOT NULL,
    "depositAddress" TEXT NOT NULL,
    "blockHeight" INTEGER NOT NULL,
    "expiryBlock" INTEGER NOT NULL,

    CONSTRAINT "SwapDepositChannelBlock_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "SwapDepositChannelBlock_blockHeight_idx" ON "public"."SwapDepositChannelBlock"("blockHeight");
