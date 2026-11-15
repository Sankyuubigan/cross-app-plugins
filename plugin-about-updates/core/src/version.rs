//! Кастомное сравнение версий релизов (НЕ semver).
//!
//! Формат версий проекта — YY.M.P (26.11.5), иногда с ведущим «v»/«V»
//! (теги GitHub Releases: v26.11.5) и prerelease-суффиксом после «-»
//! (26.12.0-beta.1). Правила:
//! - ведущий v/V срезается (v26.11.5 == 26.11.5);
//! - сегменты через «.» — u64, поэлементно, недостающие = 0 (26.09.1 == 26.9.1,
//!   26.11 == 26.11.0);
//! - prerelease меньше стабильного той же числовой базы (26.12.0-beta.1 < 26.12.0);
//! - сегменты prerelease: числа — численно, идентификаторы — лексически,
//!   число меньше идентификатора, короткий список меньше длинного.

use std::cmp::Ordering;

/// Разобрать версию на числовые сегменты и опциональный prerelease.
fn parse(v: &str) -> (Vec<u64>, Option<String>) {
    let s = v.trim();
    let s = s.strip_prefix(&['v', 'V'][..]).unwrap_or(s);
    let (base, pre) = match s.split_once('-') {
        Some((b, p)) if !b.is_empty() => (b, Some(p.to_string())),
        _ => (s, None),
    };
    let segments = base
        .split('.')
        .map(|seg| seg.trim().parse::<u64>().unwrap_or(0))
        .collect();
    (segments, pre)
}

/// Сравнить две версии релиза: `Greater` — `a` новее `b`.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (segs_a, pre_a) = parse(a);
    let (segs_b, pre_b) = parse(b);

    let len = segs_a.len().max(segs_b.len());
    for i in 0..len {
        let x = segs_a.get(i).copied().unwrap_or(0);
        let y = segs_b.get(i).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }

    // Числовые базы равны: стабильный релиз новее prerelease.
    match (&pre_a, &pre_b) {
        (None, None) => Ordering::Equal,
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (Some(x), Some(y)) => compare_prerelease(x, y),
    }
}

/// Сравнить prerelease-суффиксы («beta.1», «rc.2», «alpha»).
fn compare_prerelease(x: &str, y: &str) -> Ordering {
    let xs: Vec<&str> = x.split('.').collect();
    let ys: Vec<&str> = y.split('.').collect();
    for i in 0..xs.len().max(ys.len()) {
        match (xs.get(i), ys.get(i)) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => {
                let ord = match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(na), Ok(nb)) => na.cmp(&nb),
                    // Числовые сегменты младше текстовых (конвенция semver).
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => a.cmp(b),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::{Equal, Greater, Less};

    #[test]
    fn numeric_segments_compare_element_wise() {
        assert_eq!(compare_versions("26.8.9", "26.8.10"), Less);
        assert_eq!(compare_versions("26.8.10", "26.8.9"), Greater);
        assert_eq!(compare_versions("26.11.5", "26.11.5"), Equal);
        assert_eq!(compare_versions("26.12", "26.11.99"), Greater);
    }

    /// Ведущие нули сегментов не влияют: u64-парсинг съедает их.
    #[test]
    fn leading_zeroes_are_equal() {
        assert_eq!(compare_versions("26.09.1", "26.9.1"), Equal);
        assert_eq!(compare_versions("26.09.1", "26.09.1"), Equal);
        assert_eq!(compare_versions("026.9.1", "26.09.1"), Equal);
    }

    /// Теги GitHub Releases носят ведущий v/V — срезается ровно один.
    #[test]
    fn v_prefix_is_stripped() {
        assert_eq!(compare_versions("v26.11.5", "26.11.5"), Equal);
        assert_eq!(compare_versions("V26.11.5", "26.11.5"), Equal);
        assert_eq!(compare_versions("v26.11.5", "v26.12.0"), Less);
    }

    /// Недостающие сегменты = 0 (26.11 == 26.11.0).
    #[test]
    fn missing_segments_are_zero() {
        assert_eq!(compare_versions("26.11", "26.11.0"), Equal);
        assert_eq!(compare_versions("26.11", "26.11.1"), Less);
    }

    /// Стабильный релиз новее prerelease той же числовой базы.
    #[test]
    fn stable_is_newer_than_prerelease() {
        assert_eq!(compare_versions("26.12.0-beta.1", "26.12.0"), Less);
        assert_eq!(compare_versions("26.12.0", "26.12.0-rc.1"), Greater);
        // prerelease сравниваются между собой
        assert_eq!(compare_versions("26.12.0-beta.1", "26.12.0-beta.2"), Less);
        assert_eq!(compare_versions("26.12.0-beta.10", "26.12.0-beta.9"), Greater);
        assert_eq!(compare_versions("26.12.0-alpha", "26.12.0-beta"), Less);
        assert_eq!(compare_versions("26.12.0-1", "26.12.0-alpha"), Less);
    }

    /// Мусорные сегменты парсятся в 0, пробелы по краям — не версия.
    #[test]
    fn garbage_is_tolerated() {
        assert_eq!(compare_versions(" 26.11.5 ", "26.11.5"), Equal);
        assert_eq!(compare_versions("26.x.5", "26.0.5"), Equal);
    }
}
