use std::{cmp::Ordering, path::Path, sync::OnceLock};

use regex::Regex;

use crate::model::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct NumberKey {
    disc: u32,
    track: u32,
}

fn filename_number_key(path: &Path) -> Option<NumberKey> {
    let stem = path.file_stem()?.to_string_lossy();
    static DISC_TRACK: OnceLock<Regex> = OnceLock::new();
    static LEADING: OnceLock<Regex> = OnceLock::new();
    let disc_track = DISC_TRACK
        .get_or_init(|| Regex::new(r"^\s*(\d{1,3})[-.](\d{1,3})(?:\D|$)").unwrap());
    if let Some(captures) = disc_track.captures(&stem) {
        return Some(NumberKey {
            disc: captures[1].parse().ok()?,
            track: captures[2].parse().ok()?,
        });
    }
    let leading = LEADING
        .get_or_init(|| Regex::new(r"^\s*(\d{1,4})(?:\s|[-._]|$)").unwrap());
    leading.captures(&stem).map(|captures| NumberKey {
        disc: 0,
        track: captures[1].parse().unwrap_or(u32::MAX),
    })
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();
    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
                let mut an = String::new();
                let mut bn = String::new();
                while a_chars.peek().is_some_and(char::is_ascii_digit) {
                    an.push(a_chars.next().unwrap());
                }
                while b_chars.peek().is_some_and(char::is_ascii_digit) {
                    bn.push(b_chars.next().unwrap());
                }
                let av = an.trim_start_matches('0');
                let bv = bn.trim_start_matches('0');
                let order = av.len().cmp(&bv.len()).then_with(|| av.cmp(bv));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(_), Some(_)) => {
                let ac = a_chars.next().unwrap().to_ascii_lowercase();
                let bc = b_chars.next().unwrap().to_ascii_lowercase();
                let order = ac.cmp(&bc);
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

pub fn sort_tracks(tracks: &mut [Track]) -> bool {
    let numbered = tracks
        .iter()
        .filter(|track| filename_number_key(&track.path).is_some())
        .count();
    tracks.sort_by(|a, b| {
        match (filename_number_key(&a.path), filename_number_key(&b.path)) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => match (
                a.disc_number.zip(a.track_number),
                b.disc_number.zip(b.track_number),
            ) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => natural_cmp(&a.display_name, &b.display_name),
            },
        }
        .then_with(|| a.original_index.cmp(&b.original_index))
    });
    numbered > 0 && numbered < tracks.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_prefixes() {
        for (name, expected) in [
            ("1 Song.flac", (0, 1)),
            ("01 - Song.flac", (0, 1)),
            ("01_Song.flac", (0, 1)),
            ("1-01 Song.flac", (1, 1)),
            ("2.10 Song.flac", (2, 10)),
        ] {
            let key = filename_number_key(Path::new(name)).unwrap();
            assert_eq!((key.disc, key.track), expected, "{name}");
        }
    }

    #[test]
    fn natural_numbers_are_numeric() {
        let mut names = vec!["10.flac", "2.flac", "1.flac"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["1.flac", "2.flac", "10.flac"]);
    }
}
