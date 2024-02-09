/*
  Warnings:

  - You are about to drop the column `pairAsset` on the `Pool` table. All the data in the column will be lost.
  - A unique constraint covering the columns `[baseAsset,quoteAsset]` on the table `Pool` will be added. If there are existing duplicate values, this will fail.
  - Added the required column `quoteAsset` to the `Pool` table without a default value. This is not possible if the table is not empty.

*/
-- DropIndex
DROP INDEX "public"."Pool_baseAsset_pairAsset_key";

-- AlterTable
ALTER TABLE "public"."Pool" RENAME COLUMN "pairAsset" TO "quoteAsset";

-- CreateIndex
CREATE UNIQUE INDEX "Pool_baseAsset_quoteAsset_key" ON "public"."Pool"("baseAsset", "quoteAsset");
