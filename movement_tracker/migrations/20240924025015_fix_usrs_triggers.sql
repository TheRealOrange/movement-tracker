DROP TRIGGER IF EXISTS trigger_disable_notifications_admin_change
ON usrs;
CREATE TRIGGER trigger_disable_notifications_admin_change
    AFTER UPDATE OF admin ON usrs
    FOR EACH ROW
    WHEN (OLD.admin IS DISTINCT FROM NEW.admin)
EXECUTE PROCEDURE disable_notifications_on_admin_change();