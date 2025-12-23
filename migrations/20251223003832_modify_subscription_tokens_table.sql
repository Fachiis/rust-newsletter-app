-- Add migration script here
ALTER TABLE subscription_tokens
RENAME COLUMN subscription_tokens TO subscription_token;