//! Column-name-based heuristic generators.
//!
//! Matches well-known column names (e.g. `email`, `first_name`) and returns
//! realistic fake values from the `fake` crate.

use fake::faker::address::en::{CityName, CountryName, StateAbbr, StreetName, ZipCode};
use fake::faker::company::en::CompanyName;
use fake::faker::internet::en::{FreeEmail, SafeEmail, Username};
use fake::faker::lorem::en::{Sentence, Word, Words};
use fake::faker::name::en::{FirstName, LastName, Name};
use fake::faker::phone_number::en::PhoneNumber;
use fake::Fake;
use rand::Rng;

/// Match on common column-name patterns and return a fake value.
pub fn name_hint(col_name: &str, _rng: &mut impl Rng) -> Option<String> {
    if col_name == "email" || col_name.ends_with("_email") || col_name.starts_with("email_") {
        return Some(FreeEmail().fake::<String>());
    }
    if col_name == "safe_email" {
        return Some(SafeEmail().fake::<String>());
    }
    if col_name == "username" || col_name == "user_name" || col_name == "login" {
        return Some(Username().fake::<String>());
    }
    if col_name == "first_name" || col_name == "firstname" || col_name == "given_name" {
        return Some(FirstName().fake::<String>());
    }
    if col_name == "last_name"
        || col_name == "lastname"
        || col_name == "surname"
        || col_name == "family_name"
    {
        return Some(LastName().fake::<String>());
    }
    if col_name == "name"
        || col_name == "full_name"
        || col_name == "fullname"
        || col_name == "display_name"
    {
        return Some(Name().fake::<String>());
    }
    if col_name == "phone"
        || col_name == "phone_number"
        || col_name == "mobile"
        || col_name == "tel"
    {
        return Some(PhoneNumber().fake::<String>());
    }
    if col_name == "city" || col_name == "city_name" {
        return Some(CityName().fake::<String>());
    }
    if col_name == "country" || col_name == "country_name" {
        return Some(CountryName().fake::<String>());
    }
    if col_name == "state" || col_name == "province" || col_name == "region" {
        return Some(StateAbbr().fake::<String>());
    }
    if col_name == "zip"
        || col_name == "zip_code"
        || col_name == "postal_code"
        || col_name == "postcode"
    {
        return Some(ZipCode().fake::<String>());
    }
    if col_name.contains("street") || col_name == "address_line" || col_name == "address1" {
        return Some(StreetName().fake::<String>());
    }
    if col_name == "company"
        || col_name == "company_name"
        || col_name == "organisation"
        || col_name == "organization"
    {
        return Some(CompanyName().fake::<String>());
    }
    if col_name == "word" || col_name == "tag" || col_name == "key" {
        return Some(Word().fake::<String>());
    }
    if col_name.contains("description")
        || col_name.contains("comment")
        || col_name.contains("note")
        || col_name == "bio"
        || col_name == "summary"
        || col_name == "content"
        || col_name == "body"
        || col_name == "message"
    {
        let words: Vec<String> = Words(5..10).fake();
        return Some(words.join(" "));
    }
    if col_name == "title" || col_name == "subject" {
        let words: Vec<String> = Words(3..6).fake();
        return Some(words.join(" "));
    }
    if col_name.contains("sentence") || col_name == "text" || col_name == "excerpt" {
        return Some(Sentence(6..12).fake::<String>());
    }

    None
}
