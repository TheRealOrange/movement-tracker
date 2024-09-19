START TRANSACTION;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

DO $$ BEGIN
    CREATE TYPE user_type_enum AS ENUM ('staff', 'ns', 'active');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$ LANGUAGE plpgsql;

DO $$ BEGIN
    CREATE TYPE role_type_enum AS ENUM ('pilot', 'aro');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$ LANGUAGE plpgsql;


DO $$ BEGIN
    CREATE TYPE ict_enum AS ENUM ('live', 'sims', 'other');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$ LANGUAGE plpgsql;


CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;


DO $$ BEGIN
CREATE TABLE IF NOT EXISTS usrs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tele_id INT8 UNIQUE NOT NULL,
    name TEXT NOT NULL,
    ops_name TEXT UNIQUE NOT NULL,
    usr_type user_type_enum NOT NULL,
    role_type role_type_enum NOT NULL,
    admin BOOLEAN NOT NULL DEFAULT FALSE,
    created TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    is_valid BOOLEAN NOT NULL DEFAULT TRUE
);
DROP TRIGGER IF EXISTS users_update
ON usrs;
CREATE TRIGGER users_update
BEFORE UPDATE ON usrs
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
END $$ LANGUAGE plpgsql;


DO $$ BEGIN
CREATE TABLE IF NOT EXISTS apply (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tele_id INT8 UNIQUE NOT NULL,
    chat_username TEXT NOT NULL,
    name TEXT NOT NULL,
    ops_name TEXT UNIQUE NOT NULL,
    usr_type user_type_enum NOT NULL,
    role_type role_type_enum NOT NULL,
    created TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    is_valid BOOLEAN NOT NULL DEFAULT TRUE
);
DROP TRIGGER IF EXISTS apply_update
ON apply;
CREATE TRIGGER apply_update
BEFORE UPDATE ON apply
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
END $$ LANGUAGE plpgsql;


DO $$ BEGIN
CREATE TABLE IF NOT EXISTS availability (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    usr_id UUID REFERENCES usrs(id) NOT NULL,
    avail DATE NOT NULL,
    ict_type ict_enum NOT NULL,
    remarks TEXT,
    planned BOOLEAN NOT NULL DEFAULT FALSE,
    saf100 BOOLEAN NOT NULL DEFAULT FALSE,
    attended BOOLEAN NOT NULL DEFAULT FALSE,
    created TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    is_valid BOOLEAN NOT NULL DEFAULT TRUE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_availability_usr_date ON availability (usr_id, avail);
DROP TRIGGER IF EXISTS availability_update
ON availability;
CREATE TRIGGER availability_update
BEFORE UPDATE ON availability
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
END $$ LANGUAGE plpgsql;


DO $$ BEGIN
CREATE TABLE IF NOT EXISTS movement (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    usr_id UUID REFERENCES usrs(id) NOT NULL,
    avail DATE NOT NULL,
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,
    activity TEXT NOT NULL,
    location TEXT,
    remarks TEXT,
    created TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    is_valid BOOLEAN NOT NULL DEFAULT TRUE
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_movement_usr_date ON movement (usr_id, avail);
DROP TRIGGER IF EXISTS movement_update
ON movement;
CREATE TRIGGER movement_update
BEFORE UPDATE ON movement
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
END $$ LANGUAGE plpgsql;

DO $$ BEGIN
CREATE TABLE IF NOT EXISTS scheduled_notifications  (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    avail_id UUID REFERENCES availability(id) NOT NULL,
    scheduled_time TIMESTAMP WITH TIME ZONE NOT NULL,
    sent BOOLEAN NOT NULL DEFAULT FALSE,
    created TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    is_valid BOOLEAN NOT NULL DEFAULT TRUE
);
DROP TRIGGER IF EXISTS scheduled_notifications_update
ON scheduled_notifications;
CREATE TRIGGER scheduled_notifications_update
    BEFORE UPDATE ON scheduled_notifications
    FOR EACH ROW
    EXECUTE PROCEDURE trigger_set_timestamp();
END $$ LANGUAGE plpgsql;

DO $$ BEGIN
CREATE TABLE IF NOT EXISTS notification_settings   (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    chat_id INT8 UNIQUE NOT NULL,
    notif_system BOOLEAN NOT NULL DEFAULT FALSE,
    notif_register BOOLEAN NOT NULL DEFAULT FALSE,
    notif_availability BOOLEAN NOT NULL DEFAULT FALSE,
    notif_plan BOOLEAN NOT NULL DEFAULT FALSE,
    notif_conflict BOOLEAN NOT NULL DEFAULT FALSE,
    created TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    is_valid BOOLEAN NOT NULL DEFAULT TRUE
);
DROP TRIGGER IF EXISTS notification_settings_update
ON notification_settings;
CREATE TRIGGER notification_settings_update
    BEFORE UPDATE ON notification_settings
    FOR EACH ROW
    EXECUTE PROCEDURE trigger_set_timestamp();
END $$ LANGUAGE plpgsql;

COMMIT;