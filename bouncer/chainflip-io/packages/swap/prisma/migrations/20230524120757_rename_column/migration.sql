/*
  Warnings:

  - You are about to drop the column `blockHeight` on the `SwapDepositChannel` table. All the data in the column will be lost.
  - You are about to drop the column `blockHeight` on the `SwapDepositChannelBlock` table. All the data in the column will be lost.
  - Added the required column `issuedBlock` to the `SwapDepositChannelBlock` table without a default value. This is not possible if the table is not empty.

*/
-- DropIndex
DROP INDEX "public"."SwapDepositChannelBlock_blockHeight_idx";

-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" DROP COLUMN "blockHeight",
ADD COLUMN     "issuedBlock" INTEGER;

-- AlterTable
ALTER TABLE "public"."SwapDepositChannelBlock" DROP COLUMN "blockHeight",
ADD COLUMN     "issuedBlock" INTEGER NOT NULL;

-- CreateIndex
CREATE INDEX "SwapDepositChannelBlock_issuedBlock_expiryBlock_idx" ON "public"."SwapDepositChannelBlock"("issuedBlock", "expiryBlock");
