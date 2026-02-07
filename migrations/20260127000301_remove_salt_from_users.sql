-- migrations/20260127000301_remove_salt_from_users.sql
ALTER TABLE users DROP COLUMN salt;
