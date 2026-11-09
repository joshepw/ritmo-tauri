use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, Timelike};

const WEEKDAYS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];
const WEEKDAYS_SHORT: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
const WEEKDAYS_MENU: [&str; 7] = ["Lun", "Mar", "Mié", "Jue", "Vie", "Sáb", "Dom"];
const MONTHS: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];
const MONTHS_SHORT: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

fn weekday_index(date: NaiveDate) -> usize {
    date.weekday().num_days_from_monday() as usize
}

pub fn format_clock(time: NaiveTime) -> String {
    let hour24 = time.hour();
    let hour = match hour24 % 12 {
        0 => 12,
        value => value,
    };
    let suffix = if hour24 < 12 { "a. m." } else { "p. m." };
    format!("{hour}:{:02} {suffix}", time.minute())
}

pub fn format_event_clock(time: NaiveTime) -> String {
    let hour24 = time.hour();
    let hour = match hour24 % 12 {
        0 => 12,
        value => value,
    };
    let suffix = if hour24 < 12 { "am" } else { "pm" };
    format!("{hour}:{:02} {suffix}", time.minute())
}

pub fn format_menu_clock(now: DateTime<Local>) -> String {
    let index = weekday_index(now.date_naive());
    let month = MONTHS_SHORT[(now.month() - 1) as usize];
    format!(
        "{} {} {}  {}",
        WEEKDAYS_MENU[index],
        now.day(),
        month,
        format_clock(now.time())
    )
}

pub fn format_weekday(now: DateTime<Local>) -> String {
    let name = WEEKDAYS[weekday_index(now.date_naive())];
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn format_date_long(now: DateTime<Local>) -> String {
    let month = MONTHS[(now.month() - 1) as usize];
    format!("{} de {month}", now.day())
}

pub fn format_day_heading(today: NaiveDate, date: NaiveDate) -> String {
    let index = weekday_index(date);
    let mut name = WEEKDAYS[index].to_string();
    if let Some(first) = name.chars().next() {
        name.replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
    }
    if date.month() == today.month() && date.year() == today.year() {
        format!("{name} {}", date.day())
    } else {
        let month = MONTHS_SHORT[(date.month() - 1) as usize];
        format!("{name} {} {month}", date.day())
    }
}

pub fn format_relative_day(today: NaiveDate, date: NaiveDate) -> String {
    let days = (date - today).num_days();
    if days == 0 {
        "Hoy".into()
    } else if days == 1 {
        "Mañana".into()
    } else if days == -1 {
        "Ayer".into()
    } else if days < 0 {
        format!("Hace {} días", -days)
    } else if days < 7 {
        let index = weekday_index(date);
        let mut name = WEEKDAYS_SHORT[index].to_string();
        if let Some(first) = name.chars().next() {
            name.replace_range(..first.len_utf8(), &first.to_uppercase().to_string());
        }
        name
    } else {
        let month = MONTHS_SHORT[(date.month() - 1) as usize];
        format!("{} {month}", date.day())
    }
}

pub fn format_tray_countdown(mins: i64) -> String {
    if mins <= 0 {
        return "Ahora".into();
    }
    if mins < 60 {
        return format!("en {mins}m");
    }
    let hours = mins / 60;
    let rest = mins % 60;
    if rest == 0 {
        format!("en {hours}h")
    } else {
        format!("en {hours}h {rest}m")
    }
}

pub fn format_countdown(mins: i64) -> String {
    if mins <= 0 {
        return "Ahora".into();
    }
    if mins < 60 {
        return format!("{mins} min");
    }
    let hours = mins / 60;
    let rest = mins % 60;
    if rest == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {rest} min")
    }
}

pub fn initials(name: &str) -> String {
    let letters: String = name
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .take(2)
        .filter_map(|part| part.chars().next())
        .collect();
    let letters = letters.to_uppercase();
    if letters.is_empty() {
        "G".into()
    } else {
        letters
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 28, 13, 4, 0).unwrap()
    }

    #[test]
    fn formats_spanish_clock_and_date() {
        let now = sample();
        assert_eq!(format_clock(now.time()), "1:04 p. m.");
        assert_eq!(format_event_clock(now.time()), "1:04 pm");
        assert_eq!(
            format_event_clock(chrono::NaiveTime::from_hms_opt(9, 5, 0).unwrap()),
            "9:05 am"
        );
        assert_eq!(format_weekday(now), "Miércoles");
        assert_eq!(format_date_long(now), "28 de octubre");
        assert_eq!(format_menu_clock(now), "Mié 28 oct  1:04 p. m.");
    }

    #[test]
    fn countdown_and_initials() {
        assert_eq!(format_countdown(56), "56 min");
        assert_eq!(format_countdown(60), "1 h");
        assert_eq!(format_countdown(90), "1 h 30 min");
        assert_eq!(format_countdown(0), "Ahora");
        assert_eq!(format_tray_countdown(56), "en 56m");
        assert_eq!(format_tray_countdown(60), "en 1h");
        assert_eq!(format_tray_countdown(137), "en 2h 17m");
        assert_eq!(format_tray_countdown(0), "Ahora");
        assert_eq!(initials("Tu nombre"), "TN");
    }
}
