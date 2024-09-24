use chrono::{Datelike, Local, NaiveDate};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use rand::distributions::Alphanumeric;
use rand::Rng;
use teloxide::types::User;

//Regular expressions for parsing different date formats
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

static YEAR_MONTH_DAY_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(\d{4})\s+([A-Za-z]+)\s+(\d{1,2})\s*$")
        .expect("Failed to compile YEAR_MONTH_DAY_PATTERN regex")
});


// Mapping of month names to numbers
static MONTHS_MAP: Lazy<HashMap<&'static str, u32>> = Lazy::new(|| {
    [
        // Full names
        ("january", 1), ("february", 2), ("march", 3), ("april", 4),
        ("may", 5), ("june", 6), ("july", 7), ("august", 8),
        ("september", 9), ("october", 10), ("november", 11), ("december", 12),

        // Standard abbreviations
        ("jan", 1), ("feb", 2), ("mar", 3), ("apr", 4),
        ("may", 5), ("jun", 6), ("jul", 7), ("aug", 8),
        ("sep", 9), ("sept", 9), ("oct", 10), ("nov", 11), ("dec", 12),

        // Abbreviations with periods
        ("jan.", 1), ("feb.", 2), ("mar.", 3), ("apr.", 4),
        ("jun.", 6), ("jul.", 7), ("aug.", 8), ("sep.", 9),
        ("sept.", 9), ("oct.", 10), ("nov.", 11), ("dec.", 12),

        // Extended abbreviations (four letters)
        ("janu", 1), ("febr", 2), ("marc", 3), ("apri", 4),
        ("june", 6), ("july", 7), ("augu", 8), ("sept", 9),
        ("octo", 10), ("nove", 11), ("dece", 12)
    ]
        .iter()
        .cloned()
        .collect()
});


// Helper function to adjust year if date is in the past
fn date_or_string(
    input: &str,
    today: NaiveDate,
    mut year: i32,
    month: u32,
    day: u32,
    year_provided: bool,
) -> Result<NaiveDate, String> {
    if let Some(valid_date) = NaiveDate::from_ymd_opt(year, month, day) {
        if valid_date < today {
            if year_provided {
                Err(format!("{} is in the past.", input))
            } else {
                year += 1;
                if let Some(new_date) = NaiveDate::from_ymd_opt(year, month, day) {
                    if new_date < today {
                        return Err(format!(
                            "Adjusted date {} is still in the past.",
                            new_date
                        ));
                    }
                    Ok(new_date)
                } else {
                    Err(format!("{} is not a valid date, inferred year (guessed: {}-{}-{})", input, year, month, day))
                }
            }
        } else {
            Ok(valid_date)
        }
    } else { Err(format!("{} is not a valid date (guessed: {}-{}-{})", input, year, month, day)) }
}

// Parses a single date string into NaiveDate
pub(crate) fn parse_single_date(input: &str) -> Result<NaiveDate, String> {
    let today = Local::now().naive_local().date();
    let current_year = today.year();

    // 1. Try "year month day" pattern, e.g., "2024 nov 12"
    if let Some(caps) = YEAR_MONTH_DAY_PATTERN.captures(input) {
        let year = caps.get(1).unwrap().as_str().parse::<i32>().map_err(|_| input.to_string())?;
        let month_str = caps.get(2).unwrap().as_str().to_lowercase();
        let day = caps.get(3).unwrap().as_str().parse::<u32>().map_err(|_| input.to_string())?;

        let month = MONTHS_MAP.get(month_str.as_str()).copied().ok_or(input.to_string())?;

        date_or_string(input, today, year, month, day, true)
    }
    // 2. Try full month-first pattern with explicit year, e.g., "May 17 2024"
    else if let Some(caps) = FULL_MONTH_FIRST_PATTERN.captures(input) {
        let month_str = &caps[1];
        let day = caps[2].parse::<u32>().map_err(|_| input.to_string())?;
        let year = caps[3].parse::<i32>().unwrap_or(current_year);

        let month_str_lower = month_str.to_lowercase();
        let month = MONTHS_MAP.get(month_str_lower.as_str()).copied().ok_or(input.to_string())?;

        date_or_string(input, today, year, month, day, true)
    }
    // 3. Try day-first pattern, e.g., "28/08" or "1st Sept"
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

        let (year, provided) = if !year_str.is_empty() {
            if year_str.len() == 2 {
                // Handle two-digit years by assuming 2000+
                (year_str.parse::<i32>().map_or(current_year, |y| 2000 + y), true)
            } else {
                (year_str.parse::<i32>().unwrap_or(current_year), true)
            }
        } else {
            (current_year, false)
        };

        date_or_string(input, today, year, month, day, provided)
    }
    // 4. Try month-first pattern, e.g., "November 2nd" or "Jul 4"
    else if let Some(caps) = MONTH_FIRST_PATTERN.captures(input) {
        let month_str = &caps[1];
        let day = caps[2].parse::<u32>().map_err(|_| input.to_string())?;
        let year_str = caps.get(3).map_or("", |m| m.as_str());

        let month_str_lower = month_str.to_lowercase();
        let month = MONTHS_MAP.get(month_str_lower.as_str()).copied().ok_or(input.to_string())?;

        let (year, provided) = if !year_str.is_empty() {
            if year_str.len() == 2 {
                // Handle two-digit years by assuming 2000+
                (year_str.parse::<i32>().map_or(current_year, |y| 2000 + y), true)
            } else {
                (year_str.parse::<i32>().unwrap_or(current_year), true)
            }
        } else {
            (current_year, false)
        };

        date_or_string(input, today, year, month, day, provided)
    }
    // 5. Try numeric date formats like "09/10/2023" or "9-10-2023"
    else {
        // Attempt to parse as a numeric date
        match NaiveDate::parse_from_str(input, "%d/%m/%Y") {
            Ok(date) => Ok(date),
            Err(_) => match NaiveDate::parse_from_str(input, "%d-%m-%Y") {
                Ok(date) => Ok(date),
                Err(_) => {
                    log::debug!("Failed to parse date string: '{}'", input);
                    Err(input.to_string())
                }
            },
        }
    }
}

// Parses the input string into individual dates, handling single dates and ranges
pub(crate) fn parse_dates(input: &str) -> (Vec<NaiveDate>, Vec<String>, Vec<NaiveDate>) {
    let mut parsed_dates = Vec::new();
    let mut failed_parsing_dates = Vec::new();
    let mut duplicate_dates = Vec::new();

    for segment in input.split(',') {
        let trimmed = segment.trim();

        if trimmed.is_empty() {
            continue;
        }

        // First, attempt to parse as a single date
        match parse_single_date(trimmed) {
            Ok(date) => {
                parsed_dates.push(date);
            }
            Err(_) => {
                // If single date parsing fails, attempt to parse as a date range
                match parse_date_range(trimmed) {
                    Ok(dates) => {
                        for date in dates {
                            parsed_dates.push(date);
                        }
                    }
                    Err(_) => {
                        // If both parsing attempts fail, record the failure
                        failed_parsing_dates.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    // Deduplicate dates while preserving order
    let mut seen = HashSet::new();
    let mut unique_parsed = Vec::new();

    for date in parsed_dates {
        if !seen.insert(date) {
            duplicate_dates.push(date);
        } else {
            unique_parsed.push(date);
        }
    }

    (unique_parsed, failed_parsing_dates, duplicate_dates)
}

fn format_permutation(perm: &[&&str], required_patterns: &[&str]) -> String {
    if let Some((last, all_but_last)) = perm.split_last() {
        // Format all but the last element
        let mut formatted = all_but_last
            .iter()
            .map(|&pattern| {
                if required_patterns.contains(&pattern) {
                    format!(r"(?:{})(?:[-/\s]+|$)", pattern) // Required component
                } else {
                    format!(r"(?:{}(?:[-/\s]+|$))?", pattern) // Optional component
                }
            })
            .collect::<Vec<String>>()
            .join("");

        // Add the last element
        if required_patterns.contains(last) {
            formatted += &format!(r"(?:{})", last); // Required component
        } else {
            formatted += &format!(r"(?:{})?", last); // Optional component
        }

        formatted
    } else {
        String::new() // Handle empty permutation if necessary
    }
}

static MONTHS_MAP_RANGE: Lazy<HashMap<&'static str, u32>> = Lazy::new(|| {
    [
        // Full names
        ("january", 1), ("february", 2), ("march", 3), ("april", 4),
        ("may", 5), ("june", 6), ("july", 7), ("august", 8),
        ("september", 9), ("october", 10), ("november", 11), ("december", 12),

        // Standard abbreviations
        ("jan", 1), ("feb", 2), ("mar", 3), ("apr", 4),
        ("may", 5), ("jun", 6), ("jul", 7), ("aug", 8),
        ("sep", 9), ("sept", 9), ("oct", 10), ("nov", 11), ("dec", 12),

        // Abbreviations with periods
        ("jan.", 1), ("feb.", 2), ("mar.", 3), ("apr.", 4),
        ("jun.", 6), ("jul.", 7), ("aug.", 8), ("sep.", 9),
        ("sept.", 9), ("oct.", 10), ("nov.", 11), ("dec.", 12),

        // Extended abbreviations (four letters)
        ("janu", 1), ("febr", 2), ("marc", 3), ("apri", 4),
        ("june", 6), ("july", 7), ("augu", 8), ("sept", 9),
        ("octo", 10), ("nove", 11), ("dece", 12),

        // Numeric months as strings
        ("1", 1), ("01", 1), ("2", 2), ("02", 2), ("3", 3), ("03", 3),
        ("4", 4), ("04", 4), ("5", 5), ("05", 5), ("6", 6), ("06", 6),
        ("7", 7), ("07", 7), ("8", 8), ("08", 8), ("9", 9), ("09", 9),
        ("10", 10), ("11", 11), ("12", 12),
    ]
        .iter()
        .cloned()
        .collect()
});

static DATE_RANGE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    let start_year = r"(?P<start_year>\d{4})"; // Strictly four digits
    let end_year = r"(?P<end_year>\d{4})";
    let start_month = r"(?P<start_month>[A-Za-z]+|\d{1,2})";
    let end_month = r"(?P<end_month>[A-Za-z]+|\d{1,2})";
    let start_day = r"(?P<start_day>\d{1,2})(?:st|nd|rd|th)?";
    let end_day = r"(?P<end_day>\d{1,2})(?:st|nd|rd|th)?";
    
    // Specify which patterns are required (day fields)
    let required_start_patterns = vec![start_day];
    let required_end_patterns = vec![end_day];

    let mut all_start_perms: Vec<Vec<&&str>> = Vec::new();
    let mut all_end_perms: Vec<Vec<&&str>> = Vec::new();

    // List the permutations in order of their priority
    // Day/Month/Year
    all_start_perms.push(vec![&start_day, &start_month, &start_year]);
    all_end_perms.push(vec![&end_day, &end_month, &end_year]);
    // Year/Month/Day
    all_start_perms.push(vec![&start_year, &start_month, &start_day]);
    all_end_perms.push(vec![&end_year, &end_month, &end_day]);
    // Year/Day/Month
    all_start_perms.push(vec![&start_year, &start_day, &start_month]);
    all_end_perms.push(vec![&end_year, &end_day, &end_month]);
    // Month/Day/Year
    all_start_perms.push(vec![&start_month, &start_day, &start_year]);
    all_end_perms.push(vec![&end_month, &end_day, &end_year]);
    // Month/Year/Day
    all_start_perms.push(vec![&start_month, &start_year, &start_day]);
    all_end_perms.push(vec![&end_month, &end_year, &end_day]);
    // Day/Year/Month
    all_start_perms.push(vec![&start_day, &start_year, &start_month]);
    all_end_perms.push(vec![&end_day, &end_year, &end_month]);

    let mut full_patterns = Vec::new();
    for start_perm in all_start_perms {
        for end_perm in &all_end_perms {
            let pattern = format!(
                r"^\s*{}\s*(?:-|to|–)\s*{}\s*$",
                format_permutation(&start_perm, &required_start_patterns),
                format_permutation(&end_perm, &required_end_patterns)
            );
            full_patterns.push(pattern);
        }
    }

    // Compile all regex patterns
    full_patterns
        .iter()
        .map(|pat| Regex::new(pat).expect(&format!("Invalid regex pattern: {}", pat)))
        .collect()
});

// Parses a date range string into a Vec<NaiveDate>
pub fn parse_date_range(input: &str) -> Result<Vec<NaiveDate>, String> {
    let today = Local::now().naive_local().date();
    let current_year = today.year();

    // Vector to collect errors from each pattern
    let mut errors: Vec<String> = Vec::new();

    // Iterate through all regex patterns to find a match
    for regex in DATE_RANGE_PATTERNS.iter() {
        if let Some(caps) = regex.captures(input) {
            // Attempt to parse the captured groups within a separate scope
            let parse_result = (|| -> Result<Vec<NaiveDate>, String> {
                // Extract capture groups
                let start_year_str = caps.name("start_year").map(|m| m.as_str());
                let start_month_str = caps.name("start_month").map(|m| m.as_str());
                let start_day_str = caps.name("start_day").map(|m| m.as_str()).ok_or("Missing start day")?;
                let end_month_str = caps.name("end_month").map(|m| m.as_str());
                let end_day_str = caps.name("end_day").map(|m| m.as_str()).ok_or("Missing end day")?;
                let end_year_str = caps.name("end_year").map(|m| m.as_str());

                // Parse start day
                let start_day: u32 = start_day_str.parse().map_err(|_| { format!("Invalid start day: '{}'", start_day_str) })?;

                // Parse end day
                let end_day: u32 = end_day_str.parse().map_err(|_| { format!("Invalid end day: '{}'", end_day_str) })?;

                // Determine start month
                let start_month = if let Some(month_str) = start_month_str {
                    let month_lower = month_str.to_lowercase();
                    *MONTHS_MAP_RANGE.get(month_lower.as_str()).ok_or(format!("Invalid start month: '{}'", month_str))?
                } else if let Some(end_month_str) = end_month_str {
                    // If start month is not found, use end month to infer start month
                    let month_lower = end_month_str.to_lowercase();
                    *MONTHS_MAP_RANGE.get(month_lower.as_str()).ok_or(format!("Invalid end month: '{}'", end_month_str))?
                } else {
                    // Both not found, insufficient information to construct a date range
                    return Err("Insufficient information to determine start month".to_string());
                };

                // Determine end month
                let end_month = if let Some(month_str) = end_month_str {
                    let month_lower = month_str.to_lowercase();*MONTHS_MAP_RANGE.get(month_lower.as_str()).ok_or(format!("Invalid end month: '{}'", month_str))?
                } else {
                    // If end month is not found, use the start month
                    start_month
                };

                // Determine start year
                let (start_year, start_year_provided) = if let Some(year_str) = start_year_str {
                    (year_str.parse::<i32>().map_err(|_| { format!("Invalid start year: '{}'", year_str) })?, true)
                } else if let Some(end_year_str) = end_year_str {
                    // If start year is not found, use end year to infer start year
                    (end_year_str.parse::<i32>().map_err(|_| { format!("Invalid end year: '{}'", end_year_str) })?, false)
                } else {
                    // Both not found, assume current year
                    (current_year, false)
                };

                // Determine end year
                let (end_year, end_year_provided) = if let Some(year_str) = end_year_str {
                    (year_str.parse::<i32>().map_err(|_| { format!("Invalid end year: '{}'", year_str) })?, true)
                } else {
                    (start_year, false)
                };

                // Handle cases where the end date might be in the next year
                // For example: "Dec 30 - Jan 2"
                // If end month is less than start month, assume it's the next year
                let adjusted_end_year = if end_month < start_month { end_year + 1 } else { end_year };

                // Construct start and end dates
                let start_date = date_or_string(input, today, start_year, start_month, start_day, start_year_provided)?;

                let end_date = date_or_string(input, today, adjusted_end_year, end_month, end_day, end_year_provided)?;

                // Calculate the number of days between start and end dates
                let duration = end_date - start_date;
                let num_days = duration.num_days() as usize + 1; // +1 to include both start and end dates

                // Prevent generating an array size > 15
                if num_days > 15 {
                    return Err(format!("Date range is too large ({} days). Maximum allowed is 15 days.", num_days));
                }

                // Generate all dates in the range
                let mut dates = Vec::new();
                let mut current_date = start_date;

                while current_date <= end_date {
                    dates.push(current_date);
                    current_date = current_date
                        .succ_opt()
                        .ok_or_else(|| format!("Invalid date progression for '{}'", input))?;
                }

                Ok(dates)
            })();

            match parse_result {
                Ok(dates) => {
                    // Successfully parsed the date range, return the dates
                    return Ok(dates);
                }
                Err(err) => {
                    // Parsing failed for this regex, collect the error
                    errors.push(err.clone());
                    log::debug!("Regex '{}' matched but failed to parse: {}",regex.as_str(),err);
                    continue;
                }
            }
        }
    }

    // If none of the regex patterns matched successfully, compile detailed error messages
    if !errors.is_empty() {
        let error_messages = errors
            .iter()
            .map(|e| format!("Error: {}", e))
            .collect::<Vec<String>>()
            .join(", ");
        return Err(format!(
            "Failed to parse date range from input: '{}'. Reasons: {}",
            input, error_messages
        ));
    }

    // Fallback error if no patterns were tried (shouldn't happen)
    Err(format!(
        "Failed to parse date range from input: '{}'. No patterns were attempted.",
        input
    ))
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
        formatted.push_str(&format!("- {}\n", date_str));
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

    input.chars().fold(String::new(), |mut escaped_string, ch| {
        if special_characters.contains(&ch) {
            escaped_string.push('\\');
        }
        escaped_string.push(ch);
        escaped_string
    })
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
        .map(|username| format!("@{}", escape_special_characters(&username)))  // Use @username if available
        .unwrap_or_else(|| format!("[{}](tg://user?id={})", escape_special_characters(&user.first_name), user.id))  // Use first name and user ID if no username
}

// Generates a random prefix for callback data
pub(crate) fn generate_prefix() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect()
}