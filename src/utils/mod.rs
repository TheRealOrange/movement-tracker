use chrono::{NaiveDate, Datelike, Utc, Local};
use regex::Regex;
use std::collections::HashMap;

pub(crate)
fn parse_dates(input: &str) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    let today = Local::now().naive_utc().date();
    let current_year = today.year();

    // Regex patterns for different date formats
    let day_first_pattern = Regex::new(r"(\d{1,2})(?:st|nd|rd|th)?[-/\s]([A-Za-z]+|\d{1,2})[-/\s]?(\d{2,4})?").unwrap();
    let month_first_pattern = Regex::new(r"([A-Za-z]+)[-/\s]?(\d{1,2})(?:st|nd|rd|th)?[-/\s]?(\d{2,4})?").unwrap();
    let full_month_first_pattern = Regex::new(r"([A-Za-z]+)[-/\s](\d{1,2})(?:st|nd|rd|th)?[-/\s](\d{4})").unwrap();

    // Map of month names to numbers
    let months_map: HashMap<&str, u32> = [
        ("Jan", 1), ("January", 1), ("Feb", 2), ("February", 2), ("Mar", 3), ("March", 3),
        ("Apr", 4), ("April", 4), ("May", 5), ("Jun", 6), ("June", 6), ("Jul", 7), ("July", 7),
        ("Aug", 8), ("August", 8), ("Sep", 9), ("Sept", 9), ("September", 9), ("Oct", 10), ("October", 10),
        ("Nov", 11), ("November", 11), ("Dec", 12), ("December", 12)
    ].iter().cloned().collect();

    // Split input by commas
    let date_strings: Vec<&str> = input.split(',').map(|s| s.trim()).collect();

    // Parse each date string
    for date_str in date_strings {
        let mut parsed_successfully = false;

        // Try full month-first pattern with explicit year (e.g., May 17 2024, June 30th 2024)
        if let Some(caps) = full_month_first_pattern.captures(date_str) {
            let month_str = &caps[1];
            let day = caps[2].parse::<u32>().unwrap_or(1);
            let year = caps[3].parse::<i32>().unwrap_or(current_year);

            let month = months_map.get(month_str).copied().unwrap_or(1);

            if let Some(valid_date) = NaiveDate::from_ymd_opt(year, month, day) {
                dates.push(valid_date);
                parsed_successfully = true;
            }
        }
        // Try day-first pattern (e.g., 28/08, 1st Sept)
        else if let Some(caps) = day_first_pattern.captures(date_str) {
            let day = caps[1].parse::<u32>().unwrap_or(1);
            let month_str = &caps[2];
            let year_str = caps.get(3).map_or("", |m| m.as_str());

            let month = if let Ok(month_num) = month_str.parse::<u32>() {
                month_num
            } else {
                months_map.get(month_str).copied().unwrap_or(1)
            };

            // Use provided year or the current year
            let mut year = if !year_str.is_empty() {
                year_str.parse::<i32>().unwrap_or(current_year)
            } else {
                current_year
            };

            // Construct a date with the current year
            if let Some(mut valid_date) = NaiveDate::from_ymd_opt(year, month, day) {
                // If the date is in the past, increment the year to make it in the future
                if valid_date < today {
                    year += 1;
                    if let Some(future_date) = NaiveDate::from_ymd_opt(year, month, day) {
                        valid_date = future_date;
                    }
                }
                dates.push(valid_date);
                parsed_successfully = true;
            }
        }
        // Try month-first pattern (e.g., November 2nd, Jul 4, Feb 23)
        else if let Some(caps) = month_first_pattern.captures(date_str) {
            let month_str = &caps[1];
            let day = caps[2].parse::<u32>().unwrap_or(1);
            let year_str = caps.get(3).map_or("", |m| m.as_str());

            let month = months_map.get(month_str).copied().unwrap_or(1);

            // Use provided year or the current year
            let mut year = if !year_str.is_empty() {
                year_str.parse::<i32>().unwrap_or(current_year)
            } else {
                current_year
            };

            // Construct a date with the current year
            if let Some(mut valid_date) = NaiveDate::from_ymd_opt(year, month, day) {
                // If the date is in the past, increment the year to make it in the future
                if valid_date < today {
                    year += 1;
                    if let Some(future_date) = NaiveDate::from_ymd_opt(year, month, day) {
                        valid_date = future_date;
                    }
                }
                dates.push(valid_date);
                parsed_successfully = true;
            }
        }

        // If no valid pattern matched or parsing failed, log the invalid string
        if !parsed_successfully {
            log::debug!("Failed to parse date string: '{}'", date_str);
        }
    }

    // Return the parsed dates
    dates
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