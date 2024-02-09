/*
  Warnings:

  - Added the required column `type` to the `Swap` table without a default value. This is not possible if the table is not empty.

*/
-- CreateEnum
CREATE TYPE "public"."SwapType" AS ENUM ('SWAP', 'PRINCIPAL', 'GAS');

-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "type" "public"."SwapType" NOT NULL;
