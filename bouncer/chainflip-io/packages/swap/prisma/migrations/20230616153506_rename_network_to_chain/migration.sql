/*
  Warnings:

  - You are about to drop the column `network` on the `Broadcast` table. All the data in the column will be lost.
  - You are about to drop the column `network` on the `Egress` table. All the data in the column will be lost.
  - A unique constraint covering the columns `[nativeId,chain]` on the table `Broadcast` will be added. If there are existing duplicate values, this will fail.
  - A unique constraint covering the columns `[nativeId,chain]` on the table `Egress` will be added. If there are existing duplicate values, this will fail.
  - Added the required column `chain` to the `Broadcast` table without a default value. This is not possible if the table is not empty.
  - Added the required column `chain` to the `Egress` table without a default value. This is not possible if the table is not empty.

*/
-- CreateEnum
CREATE TYPE "public"."Chain" AS ENUM ('Polkadot', 'Ethereum', 'Bitcoin');

-- DropIndex
DROP INDEX "public"."Broadcast_nativeId_network_key";

-- DropIndex
DROP INDEX "public"."Egress_nativeId_network_key";

-- AlterTable
ALTER TABLE "public"."Broadcast" DROP COLUMN "network",
ADD COLUMN     "chain" "public"."Chain" NOT NULL;

-- AlterTable
ALTER TABLE "public"."Egress" DROP COLUMN "network",
ADD COLUMN     "chain" "public"."Chain" NOT NULL;

-- DropEnum
DROP TYPE "public"."Network";

-- CreateIndex
CREATE UNIQUE INDEX "Broadcast_nativeId_chain_key" ON "public"."Broadcast"("nativeId", "chain");

-- CreateIndex
CREATE UNIQUE INDEX "Egress_nativeId_chain_key" ON "public"."Egress"("nativeId", "chain");
