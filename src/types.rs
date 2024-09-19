use std::fmt;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgHasArrayType, PgTypeInfo};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};
use strum_macros::{AsRefStr, EnumIter, EnumString};
use sqlx::types::Uuid;

#[derive(Clone, Debug, sqlx::Type, Eq, PartialEq, Serialize, Deserialize, EnumString, EnumIter, AsRefStr)]
#[sqlx(type_name = "user_type_enum", rename_all = "lowercase")]
#[strum(serialize_all = "UPPERCASE")]
pub(crate) enum UsrType {
    ACTIVE,
    STAFF,
    NS,
}

#[derive(Clone, Debug, sqlx::Type, Eq, PartialEq, Serialize, Deserialize, EnumString, EnumIter, AsRefStr)]
#[sqlx(type_name = "role_type_enum", rename_all = "lowercase")]
#[strum(serialize_all = "UPPERCASE")]
pub(crate) enum RoleType {
    PILOT,
    ARO
}

#[derive(Clone, Debug, sqlx::Type, Eq, PartialEq, Serialize, Deserialize, EnumString, EnumIter, AsRefStr)]
#[sqlx(type_name = "ict_enum", rename_all = "lowercase")]
#[strum(serialize_all = "UPPERCASE")]
pub(crate) enum Ict {
    LIVE,
    SIMS,
    OTHER,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct Usr {
    pub id: Uuid,
    pub tele_id: i64,
    pub name: String,
    pub ops_name: String,
    pub usr_type: UsrType,
    pub role_type: RoleType,
    pub admin: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct UsrWithAvailability {
    pub id: Uuid,
    pub tele_id: i64,
    pub name: String,
    pub ops_name: String,
    pub usr_type: UsrType,
    pub role_type: RoleType,
    pub admin: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub availability: Vec<Availability>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct Apply {
    pub id: Uuid,
    pub tele_id: i64,
    pub chat_username: String,
    pub name: String,
    pub ops_name: String,
    pub usr_type: UsrType,
    pub role_type: RoleType,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct Availability {
    pub id: Uuid,
    pub user_id: Uuid,
    pub avail: NaiveDate,
    pub ict_type: Ict,
    pub remarks: Option<String>,
    pub planned: bool,
    pub saf100: bool,
    pub attended: bool,
    pub is_valid: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct AvailabilityDetails {
    pub id: Uuid,
    pub ops_name: String,
    pub usr_type: UsrType,
    pub avail: NaiveDate,
    pub ict_type: Ict,
    pub remarks: Option<String>,
    pub planned: bool,
    pub saf100: bool,
    pub attended: bool,
    pub is_valid: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl PgHasArrayType for Availability {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_availability")
    }
}
