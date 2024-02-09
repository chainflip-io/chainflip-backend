/*
  Warnings:

  - Added the required column `depositAsset` to the `Swap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `destinationAddress` to the `Swap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `destinationAsset` to the `Swap` table without a default value. This is not possible if the table is not empty.

*/
-- DropForeignKey
ALTER TABLE "public"."Swap" DROP CONSTRAINT "Swap_swapDepositChannelId_fkey";

-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "depositAsset" TEXT NOT NULL,
ADD COLUMN     "destinationAddress" TEXT NOT NULL,
ADD COLUMN     "destinationAsset" TEXT NOT NULL,
ALTER COLUMN "swapDepositChannelId" DROP NOT NULL;

-- AddForeignKey
ALTER TABLE "public"."Swap" ADD CONSTRAINT "Swap_swapDepositChannelId_fkey" FOREIGN KEY ("swapDepositChannelId") REFERENCES "public"."SwapDepositChannel"("id") ON DELETE SET NULL ON UPDATE CASCADE;
