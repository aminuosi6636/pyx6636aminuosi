use super::error::AppError;

pub fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

pub fn filename(base: &str, count: Option<u32>, extension: &str) -> Result<String, AppError> {
    let stamp = base
        .get(..19)
        .ok_or_else(|| AppError::review("invalid timestamp"))?;
    if chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H-%M-%S").is_err()
        || !["mp4", "mov", "m4v", "avi", "mkv"].contains(&extension)
        || base
            .chars()
            .any(|c| !(c.is_ascii_digit() || c == '-' || c == '_'))
    {
        return Err(AppError::review("invalid filename base"));
    }
    Ok(match count {
        Some(count) => format!("{base}_使用次数：{count}.{extension}"),
        None => format!("{base}.{extension}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_and_absolute_usage_names() {
        assert_eq!(
            filename("2026-09-28_14-35-08", None, "mp4").unwrap(),
            "2026-09-28_14-35-08.mp4"
        );
        assert_eq!(
            filename("2026-09-28_14-35-08_002", Some(12), "mov").unwrap(),
            "2026-09-28_14-35-08_002_使用次数：12.mov"
        );
        assert!(filename("2026-02-30_14-35-08", None, "mp4").is_err());
    }
}
