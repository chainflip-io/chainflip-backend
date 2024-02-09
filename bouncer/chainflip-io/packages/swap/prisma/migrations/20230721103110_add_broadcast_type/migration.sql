-- CreateEnum
CREATE TYPE "public"."BroadcastType" AS ENUM ('BATCH', 'CCM');

-- AlterTable
ALTER TABLE "public"."Broadcast" ADD COLUMN     "type" "public"."BroadcastType" NOT NULL DEFAULT 'BATCH';
