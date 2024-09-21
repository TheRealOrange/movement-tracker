use chrono::{Datelike, Local, NaiveDate};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use teloxide::types::User;

static FULL_MONTH_FIRST_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*([A-Za-z]+)[-/\s]+(\d{1,2})(?:st|nd|rd|th)?[-/\s]+(\d{4})\s*$")
        .expect("Failed to compile FULL_MONTH_FIRST_PATTERN regex")
});

static DAY_FIRST_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(\d{1,2})(?:st|nd|rd|th)?[-/\s]+([A-Za-z]+|\d{1,2})(?:[-/\s]+(\d{2,4}))?\s*$")
        .expect("Failed to compile DAY_FIRST_PATTERN regex")
});

static MONTH_FIRST_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*([A-Za-z]+)[-/\s]+(\d{1,2})(?:st|nd|rd|th)?(?:[-/\s]+(\d{2,4}))?\s*$")
        .expect("Failed to compile MONTH_FIRST_PATTERN regex")
});

static MONTHS_MAP: Lazy<HashMap<&'static str, u32>> = Lazy::new(|| {
    [
        ("jan", 1), ("january", 1), ("feb", 2), ("february", 2),
        ("mar", 3), ("march", 3), ("apr", 4), ("april", 4),
        ("may", 5), ("jun", 6), ("june", 6), ("jul", 7),
        ("july", 7), ("aug", 8), ("august", 8), ("sep", 9),
        ("sept", 9), ("september", 9), ("oct", 10), ("october", 10),
        ("nov", 11), ("november", 11), ("dec", 12), ("december", 12),
    ]
        .iter()
        .cloned()
        .collect()
});

fn date_or_string(input: &str, today: NaiveDate, mut year: i32, month: u32, day: u32) -> Result<NaiveDate, String> {
    if let Some(mut valid_date) = NaiveDate::from_ymd_opt(year, month, day) {
        if valid_date < today {
            year += 1;
            valid_date = NaiveDate::from_ymd_opt(year, month, day).ok_or(input.to_string())?;
        }
        Ok(valid_date)
    } else {
        Err(input.to_string())
    }
}

pub(crate) fn parse_single_date(input: &str) -> Result<NaiveDate, String> {
    let today = Local::now().naive_local().date();
    let current_year = today.year();

    // Try full month-first pattern with explicit year (e.g., May 17 2024, June 30th 2024)
    if let Some(caps) = FULL_MONTH_FIRST_PATTERN.captures(input) {
        let month_str = &caps[1];
        let day = caps[2].parse::<u32>().map_err(|_| input.to_string())?;
        let year = caps[3].parse::<i32>().unwrap_or(current_year);

        let month_str_lower = month_str.to_lowercase();
        let month = MONTHS_MAP.get(month_str_lower.as_str()).copied().ok_or(input.to_string())?;

        NaiveDate::from_ymd_opt(year, month, day).ok_or(input.to_string())
    }
    // Try day-first pattern (e.g., 28/08, 1st Sept)
    else if let Some(caps) = DAY_FIRST_PATTERN.captures(input) {
        let day = caps[1].parse::<u32>().map_err(|_| input.to_string())?;
        let month_str = &caps[2];
        let year_str = caps.get(3).map_or("", |m| m.as_str());

        let month = if let Ok(month_num) = month_str.parse::<u32>() {
            month_num
        } else {
            let month_str_lower = month_str.to_lowercase();
            MONTHS_MAP.get(month_str_lower.as_str()).copied().ok_or(input.to_string())?
        };

        let year = if !year_str.is_empty() {
            year_str.parse::<i32>().unwrap_or(current_year)
        } else {
            current_year
        };

        date_or_string(input, today, year, month, day)
    }
    // Try month-first pattern (e.g., November 2nd, Jul 4, Feb 23)
    else if let Some(caps) = MONTH_FIRST_PATTERN.captures(input) {
        let month_str = &caps[1];
        let day = caps[2].parse::<u32>().map_err(|_| input.to_string())?;
        let year_str = caps.get(3).map_or("", |m| m.as_str());

        let month_str_lower = month_str.to_lowercase();
        let month = MONTHS_MAP.get(month_str_lower.as_str()).copied().ok_or(input.to_string())?;

        let year = if !year_str.is_empty() {
            year_str.parse::<i32>().unwrap_or(current_year)
        } else {
            current_year
        };

        date_or_string(input, today, year, month, day)
    } else {
        log::debug!("Failed to parse date string: '{}'", input);
        Err(input.to_string())
    }
}

pub(crate) fn parse_dates(input: &str) -> (Vec<NaiveDate>, Vec<String>) {
    input
        .split(',')
        .map(|s| s.trim())
        .map(|s| parse_single_date(s))
        .fold((Vec::new(), Vec::new()), |mut acc, res| {
            match res {
                Ok(date) => acc.0.push(date),
                Err(failed) => acc.1.push(failed),
            }
            acc
        })
}

pub(crate) fn format_dates_as_markdown(dates: &Vec<NaiveDate>) -> String {
    let mut markdown_list = String::new();

    for date in dates {
        // Format the date in two formats: `2024 May 05`
        let formatted_date_long = date.format("%Y %b %d").to_string();

        // Append to the markdown string as a list item
        markdown_list.push_str(&format!("- {}\n", formatted_date_long));
    }

    markdown_list
}

pub(crate) fn format_failed_dates_as_markdown(failed_dates: &[String]) -> String {
    let mut formatted = String::new();
    for date_str in failed_dates {
        // Escape special characters to prevent Markdown parsing issues
        let escaped_str = escape_special_characters(date_str);
        formatted.push_str(&format!("- {}\n", escaped_str));
    }
    formatted
}

pub(crate) fn add_month_safe(date: NaiveDate, months: u32) -> NaiveDate {
    // Try to add month while considering the possibility of overflows (e.g., from January 31st to February)
    let next_month = NaiveDate::from_ymd_opt(date.year() + ((date.month() - 1 + months)/12) as i32, (date.month()-1 + months) % 12 +1, date.day());

    // Handle overflow by taking the last valid day of the next month if needed
    next_month.unwrap_or_else(|| {
        let last_day_of_next_month = NaiveDate::from_ymd_opt(date.year() + ((date.month() + months)/12) as i32, (date.month() + months) % 12 +1, 1).and_then(|d| d.pred_opt()).expect("Invalid date");
        last_day_of_next_month
    })
}

pub(crate) fn escape_special_characters(input: &str) -> String {
    let special_characters = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];

    let mut escaped_string = input.to_string();

    for &ch in &special_characters {
        let ch_str = ch.to_string();
        let escaped_ch = format!("\\{}", ch);
        escaped_string = escaped_string.replace(&ch_str, &escaped_ch);
    }

    escaped_string
}

pub(crate) const MAX_NAME_LENGTH: usize = 64;
pub(crate) const MAX_OPS_NAME_LENGTH: usize = 10;

pub(crate) fn is_valid_name(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_alphabetic() || c.is_whitespace())
}

pub(crate) fn is_valid_ops_name(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_alphabetic() || c.is_whitespace())
}

pub(crate) fn cleanup_name(name: &str) -> String {
    // Trim the input (left and right)
    let trimmed_name = name.trim();

    // Replace multiple spaces with a single space
    let single_spaced_name = trimmed_name
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    single_spaced_name
}

pub(crate) fn username_link_tag(user: &User) -> String{
    user.username
        .as_deref()
        .map(|username| format!("@{}", username))  // Use @username if available
        .unwrap_or_else(|| format!("[{}](tg://user?id={})", escape_special_characters(&user.first_name), user.id))  // Use first name and user ID if no username
}