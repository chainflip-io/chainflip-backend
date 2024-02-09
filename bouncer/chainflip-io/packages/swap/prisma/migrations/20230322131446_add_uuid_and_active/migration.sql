/*
  Warnings:

  - A unique constraint covering the columns `[uuid]` on the table `SwapIntent` will be added. If there are existing duplicate values, this will fail.
  - The required column `uuid` was added to the `SwapIntent` table with a prisma-level default value. This is not possible if the table is not empty. Please add this column as optional, then populate it before making it required.

*/
-- DropIndex
DROP INDEX "SwapIntent_ingressAddress_key";

-- AlterTable
ALTER TABLE "Swap" ALTER COLUMN "updatedAt" SET DEFAULT CURRENT_TIMESTAMP;

-- AlterTable
ALTER TABLE "SwapIntent" ADD COLUMN     "active" BOOLEAN NOT NULL DEFAULT true,
ADD COLUMN     "uuid" TEXT NOT NULL;

-- CreateIndex
CREATE UNIQUE INDEX "SwapIntent_uuid_key" ON "SwapIntent"("uuid");

-- CreateIndex
CREATE INDEX "SwapIntent_ingressAddress_idx" ON "SwapIntent"("ingressAddress");
