CREATE TABLE "Tenant" (id int);
INSERT INTO "Tenant" (id) VALUES (1);
UPDATE "Branch" SET name = 'x';
ALTER TABLE "Tenant" ADD COLUMN name text;
