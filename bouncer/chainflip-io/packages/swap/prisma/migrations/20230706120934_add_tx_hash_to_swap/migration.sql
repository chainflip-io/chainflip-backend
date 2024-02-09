/*
  Warnings:

  - A unique constraint covering the columns `[txHash]` on the table `Swap` will be added. If there are existing duplicate values, this will fail.

*/
-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "txHash" TEXT;

-- CreateIndex
CREATE UNIQUE INDEX "Swap_txHash_key" ON "public"."Swap"("txHash");
