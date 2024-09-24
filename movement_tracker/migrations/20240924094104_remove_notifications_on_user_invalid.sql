-- Trigger function to invalidate all scheduled notifications when a user is marked invalid
CREATE OR REPLACE FUNCTION invalidate_scheduled_notifications_on_user_invalid()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.is_valid = TRUE AND NEW.is_valid = FALSE THEN
        UPDATE scheduled_notifications
        SET is_valid = FALSE
        WHERE avail_id IN (
            SELECT id FROM availability
            WHERE usr_id = OLD.id
        )
        AND is_valid = TRUE;

        RAISE NOTICE 'Invalidated all scheduled notifications for user: % due to user invalidation.', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to run whenever the is_valid column of user data is updated
DROP TRIGGER IF EXISTS trigger_invalidate_scheduled_notifications_on_user_invalid
ON usrs;
CREATE TRIGGER trigger_invalidate_scheduled_notifications_on_user_invalid
    AFTER UPDATE OF is_valid ON usrs
    FOR EACH ROW
    WHEN (OLD.is_valid IS DISTINCT FROM NEW.is_valid)
EXECUTE PROCEDURE invalidate_scheduled_notifications_on_user_invalid();

