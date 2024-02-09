/*
  Warnings:

  - The values [Lifi,Squid] on the enum `ThirdPartyProtocol` will be removed. If these variants are still used in the database, this will fail.

*/
-- AlterEnum
BEGIN;
CREATE TYPE "public"."ThirdPartyProtocol_new" AS ENUM ('lifi', 'squid');
ALTER TABLE "public"."ThirdPartySwap" ALTER COLUMN "protocol" TYPE "public"."ThirdPartyProtocol_new" USING ("protocol"::text::"public"."ThirdPartyProtocol_new");
ALTER TYPE "public"."ThirdPartyProtocol" RENAME TO "ThirdPartyProtocol_old";
ALTER TYPE "public"."ThirdPartyProtocol_new" RENAME TO "ThirdPartyProtocol";
DROP TYPE "public"."ThirdPartyProtocol_old";
COMMIT;
