/*
  Warnings:

  - Changed the type of `srcAsset` on the `Swap` table. No cast exists, the column would be dropped and recreated, which cannot be done if there is data, since the column is required.
  - Changed the type of `destAsset` on the `Swap` table. No cast exists, the column would be dropped and recreated, which cannot be done if there is data, since the column is required.
  - Changed the type of `srcAsset` on the `SwapDepositChannel` table. No cast exists, the column would be dropped and recreated, which cannot be done if there is data, since the column is required.
  - Changed the type of `destAsset` on the `SwapDepositChannel` table. No cast exists, the column would be dropped and recreated, which cannot be done if there is data, since the column is required.

*/
-- CreateEnum
CREATE TYPE "public"."Asset" AS ENUM ('FLIP', 'USDC', 'DOT', 'ETH', 'BTC');

-- AlterTable
ALTER TABLE "public"."Swap" DROP COLUMN "srcAsset",
ADD COLUMN     "srcAsset" "public"."Asset" NOT NULL,
DROP COLUMN "destAsset",
ADD COLUMN     "destAsset" "public"."Asset" NOT NULL;

-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" DROP COLUMN "srcAsset",
ADD COLUMN     "srcAsset" "public"."Asset" NOT NULL,
DROP COLUMN "destAsset",
ADD COLUMN     "destAsset" "public"."Asset" NOT NULL;
