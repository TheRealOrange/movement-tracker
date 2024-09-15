use std::fmt;
use enum_iterator::Sequence;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgHasArrayType, PgTypeInfo};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

#[derive(Clone, Debug, sqlx::Type, Serialize, Deserialize, Sequence)]
#[sqlx(type_name = "user_type_enum", rename_all = "lowercase")]
pub(crate) enum UsrType {
    ACTIVE,
    STAFF,
    NS,
}

impl PgHasArrayType for UsrType {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_user_type_enum")
    }
}

impl fmt::Display for UsrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = match self {
            UsrType::ACTIVE => "ACTIVE",
            UsrType::STAFF => "STAFF",
            UsrType::NS => "NS"
        };
        write!(f, "{}", out)
    }
}

// Implement Into<String> for owned UsrType
impl Into<String> for UsrType {
    fn into(self) -> String {
        self.to_string()
    }
}

// Implement Into<String> for borrowed &UsrType
impl Into<String> for &UsrType {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Clone, Debug, sqlx::Type, Serialize, Deserialize, Sequence)]
#[sqlx(type_name = "role_type_enum", rename_all = "lowercase")]
pub(crate) enum RoleType {
    PILOT,
    ARO
}

impl PgHasArrayType for RoleType {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_role_type_enum")
    }
}

impl fmt::Display for RoleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = match self {
            RoleType::PILOT => "PILOT",
            RoleType::ARO => "ARO",
        };
        write!(f, "{}", out)
    }
}

// Implement Into<String> for owned RoleType
impl Into<String> for RoleType {
    fn into(self) -> String {
        self.to_string()
    }
}

// Implement Into<String> for borrowed &RoleType
impl Into<String> for &RoleType {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Clone, Debug, sqlx::Type, Serialize, Deserialize, Sequence)]
#[sqlx(type_name = "ict_enum", rename_all = "lowercase")]
pub(crate) enum Ict {
    LIVE,
    SIMS,
    OTHER,
}

impl PgHasArrayType for &Ict {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("_ict_enum")
    }
}

// Implement the Display trait for RoleType
impl fmt::Display for Ict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = match self {
            Ict::LIVE => "LIVE",
            Ict::SIMS => "SIMS",
            Ict::OTHER => "OTHER"
        };
        write!(f, "{}", out)
    }
}

// Implement Into<String> for owned Ict
impl Into<String> for Ict {
    fn into(self) -> String {
        self.to_string()
    }
}

// Implement Into<String> for borrowed &Ict
impl Into<String> for &Ict {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct Usr {
    pub id: i32,
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
    pub id: i32,
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
    id: i32,
    tele_id: i64,
    name: String,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
}

#[derive(Clone, sqlx::FromRow, Debug)]
pub(crate) struct Availability {
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
pub(crate) struct AvailabilityDetails {
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

// taken from https://stackoverflow.com/questions/74491204/how-do-i-represent-an-i64-in-the-u64-domain
// to convert u64 telegram ids to i64 to store inside postgres

pub fn wrap_to_u64(x: i64) -> u64 {
    (x as u64).wrapping_add(u64::MAX/2 + 1)
}
pub fn wrap_to_i64(x: u64) -> i64 {
    x.wrapping_sub(u64::MAX/2 + 1) as i64
}
