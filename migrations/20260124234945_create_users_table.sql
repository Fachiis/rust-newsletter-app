-- migrations/20260124234945_create_users_table.sql
CREATE TABLE users (
    user_id  uuid PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password TEXT NOT NULL
);