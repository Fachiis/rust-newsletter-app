-- Create Subscription Tokens Table
-- This migration creates the 'subscription_tokens' table to store tokens associated with user subscriptions.
CREATE TABLE subscription_tokens (
	subscription_tokens TEXT NOT NULL,
	subscriber_id uuid NOT NULL
		REFERENCES subscriptions(id),
    PRIMARY KEY (subscription_tokens)
);
