/*
  Warnings:

  - You are about to drop the column `sharedSecret` on the `MarketMaker` table. All the data in the column will be lost.
  - Added the required column `publicKey` to the `MarketMaker` table without a default value. This is not possible if the table is not empty.

*/
-- DropIndex
DROP INDEX "private"."MarketMaker_sharedSecret_key";

-- AlterTable
ALTER TABLE "private"."MarketMaker" DROP COLUMN "sharedSecret",
ADD COLUMN     "publicKey" TEXT NOT NULL;
