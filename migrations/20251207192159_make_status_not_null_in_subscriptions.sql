-- We wrap the whole migration in a transaction to make sure
-- it succeeds or fails as a whole.
BEGIN;
	-- First, we update all existing NULL values to 'confirmed'
	UPDATE subscriptions
		SET status = 'confirmed'
		WHERE status IS NULL;
	-- Then, we alter the column to set it as NOT NULL
	ALTER TABLE subscriptions ALTER COLUMN status SET NOT NULL;
COMMIT;
