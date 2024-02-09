/*
  Warnings:

  - You are about to drop the column `egressCompleteAt` on the `Swap` table. All the data in the column will be lost.
  - You are about to drop the column `ingressAmount` on the `Swap` table. All the data in the column will be lost.
  - You are about to drop the column `ingressReceivedAt` on the `Swap` table. All the data in the column will be lost.
  - You are about to drop the column `swapIntentId` on the `Swap` table. All the data in the column will be lost.
  - You are about to drop the column `ingressAddress` on the `SwapIntentBlock` table. All the data in the column will be lost.
  - You are about to drop the `SwapIntent` table. If the table is not empty, all the data it contains will be lost.
  - Added the required column `depositAmount` to the `Swap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `depositReceivedAt` to the `Swap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `swapDepositChannelId` to the `Swap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `depositAddress` to the `SwapIntentBlock` table without a default value. This is not possible if the table is not empty.

*/
-- DropForeignKey
ALTER TABLE "public"."Swap" DROP CONSTRAINT "Swap_swapIntentId_fkey";

-- AlterTable
ALTER TABLE "public"."Swap" DROP COLUMN "egressCompleteAt",
DROP COLUMN "ingressAmount",
DROP COLUMN "ingressReceivedAt",
DROP COLUMN "swapIntentId",
ADD COLUMN     "depositAmount" DECIMAL(30,0) NOT NULL,
ADD COLUMN     "depositReceivedAt" TIMESTAMP(3) NOT NULL,
ADD COLUMN     "egressCompletedAt" TIMESTAMP(3),
ADD COLUMN     "swapDepositChannelId" BIGINT NOT NULL;

-- AlterTable
ALTER TABLE "public"."SwapIntentBlock" DROP COLUMN "ingressAddress",
ADD COLUMN     "depositAddress" TEXT NOT NULL;

-- DropTable
DROP TABLE "public"."SwapIntent";

-- CreateTable
CREATE TABLE "public"."SwapDepositChannel" (
    "id" BIGSERIAL NOT NULL,
    "uuid" TEXT NOT NULL,
    "depositAsset" TEXT NOT NULL,
    "depositAddress" TEXT NOT NULL,
    "expectedDepositAmount" DECIMAL(30,0) NOT NULL,
    "destinationAsset" TEXT NOT NULL,
    "destinationAddress" TEXT NOT NULL,
    "active" BOOLEAN NOT NULL DEFAULT true,
    "blockHeight" INTEGER,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "SwapDepositChannel_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "SwapDepositChannel_uuid_key" ON "public"."SwapDepositChannel"("uuid");

-- CreateIndex
CREATE INDEX "SwapDepositChannel_depositAddress_idx" ON "public"."SwapDepositChannel"("depositAddress");

-- AddForeignKey
ALTER TABLE "public"."Swap" ADD CONSTRAINT "Swap_swapDepositChannelId_fkey" FOREIGN KEY ("swapDepositChannelId") REFERENCES "public"."SwapDepositChannel"("id") ON DELETE RESTRICT ON UPDATE CASCADE;
