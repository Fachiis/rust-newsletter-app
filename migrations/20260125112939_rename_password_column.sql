-- migrations/20260125112939_rename_password_column.sql
ALTER TABLE users RENAME password TO password_hash;
