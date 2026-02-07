-- migrations/20260125144426_add_salt_to_users.sql
ALTER TABLE users ADD COLUMN  salt TEXT NOT NULL;