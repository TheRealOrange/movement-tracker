use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgHasArrayType, PgTypeInfo};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

#[derive(Clone, Debug, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "usrtype", rename_all = "lowercase")]
pub enum UsrType {
    ACTIVE,
    STAFF,
    NS,
}

impl PgHasArrayType for UsrType {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_usrtype")
    }
}

#[derive(Clone, Debug, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "ict", rename_all = "lowercase")]
pub(super) enum Ict {
    LIVE,
    SIMS,
    OTHER,
}

impl PgHasArrayType for Ict {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_ict")
    }
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(super) struct Usr {
    pub id: i32,
    pub tele_id: i64,
    pub name: String,
    pub ops_name: String,
    pub usr_type: UsrType,
    pub admin: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(super) struct UsrWithAvailability {
    pub id: i32,
    pub tele_id: i64,
    pub name: String,
    pub ops_name: String,
    pub usr_type: UsrType,
    pub admin: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub availability: Vec<Availability>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(super) struct Apply {
    id: i32,
    tele_id: i64,
    name: String,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(super) struct Availability {
    id: i32,
    user_id: i32,
    avail: NaiveDate,
    ict_type: Ict,
    remarks: Option<String>,
    saf100: bool,
    attended: bool,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(super) struct AvailabilityDetails {
    pub ops_name: String,
    pub avail: NaiveDate,
    pub ict_type: Ict,
    pub remarks: Option<String>,
    pub saf100: bool,
    pub attended: bool,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl PgHasArrayType for Availability {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_availability")
    }
}
