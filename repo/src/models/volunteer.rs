use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::{qualifications, volunteers};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = volunteers)]
pub struct Volunteer {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub full_name: String,
    pub contact_email: Option<String>,
    pub contact_phone: Option<String>,
    pub gov_id_encrypted: Option<Vec<u8>>,
    pub gov_id_last4: Option<String>,
    pub private_notes_encrypted: Option<Vec<u8>>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = volunteers)]
pub struct NewVolunteer {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub full_name: String,
    pub contact_email: Option<String>,
    pub contact_phone: Option<String>,
    pub gov_id_encrypted: Option<Vec<u8>>,
    pub gov_id_last4: Option<String>,
    pub private_notes_encrypted: Option<Vec<u8>>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = qualifications)]
pub struct Qualification {
    pub id: Uuid,
    pub volunteer_id: Uuid,
    pub kind: String,
    pub issuer: String,
    pub certificate_encrypted: Option<Vec<u8>>,
    pub certificate_last4: Option<String>,
    pub issued_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = qualifications)]
pub struct NewQualification {
    pub id: Uuid,
    pub volunteer_id: Uuid,
    pub kind: String,
    pub issuer: String,
    pub certificate_encrypted: Option<Vec<u8>>,
    pub certificate_last4: Option<String>,
    pub issued_on: NaiveDate,
    pub expires_on: Option<NaiveDate>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}
