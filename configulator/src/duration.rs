use std::fmt;
use std::str::FromStr;
use std::time::Duration as StdDuration;

/// A [`std::time::Duration`] wrapper that parses and displays Go-style
/// duration strings: `"30s"`, `"1h30m"`, `"500ms"`, `"1.5h"`.
///
/// Units: `ns`, `us`/`µs`, `ms`, `s`, `m`, `h`. Concatenation and decimal
/// fractions are supported, matching Go's `time.ParseDuration`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(pub StdDuration);

impl Duration {
    /// The wrapped [`std::time::Duration`].
    pub fn get(&self) -> StdDuration {
        self.0
    }
}

impl From<StdDuration> for Duration {
    fn from(d: StdDuration) -> Self {
        Duration(d)
    }
}

impl From<Duration> for StdDuration {
    fn from(d: Duration) -> Self {
        d.0
    }
}

impl std::ops::Deref for Duration {
    type Target = StdDuration;
    fn deref(&self) -> &StdDuration {
        &self.0
    }
}

impl FromStr for Duration {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let orig = s;
        let s = s.strip_prefix('+').unwrap_or(s);
        if s.starts_with('-') {
            return Err(format!("negative duration {orig:?} is not supported"));
        }
        if s == "0" {
            return Ok(Duration(StdDuration::ZERO));
        }
        if s.is_empty() {
            return Err(format!("invalid duration {orig:?}"));
        }
        let invalid = || format!("invalid duration {orig:?}");
        let overflow = || format!("duration {orig:?} is too large");
        let mut total: u128 = 0;
        let mut rest = s;
        while !rest.is_empty() {
            let num_end = rest
                .find(|c: char| !c.is_ascii_digit() && c != '.')
                .ok_or_else(|| format!("missing unit in duration {orig:?}"))?;
            let (int, frac) = rest[..num_end]
                .split_once('.')
                .unwrap_or((&rest[..num_end], ""));
            if (int.is_empty() && frac.is_empty()) || frac.contains('.') {
                return Err(invalid());
            }
            rest = &rest[num_end..];
            let (unit_len, unit): (usize, u128) = if rest.starts_with("ns") {
                (2, 1)
            } else if rest.starts_with("us")
                || rest.starts_with("\u{b5}s")
                || rest.starts_with("\u{3bc}s")
            {
                (rest.find('s').unwrap() + 1, 1_000)
            } else if rest.starts_with("ms") {
                (2, 1_000_000)
            } else if rest.starts_with('s') {
                (1, 1_000_000_000)
            } else if rest.starts_with('m') {
                (1, 60_000_000_000)
            } else if rest.starts_with('h') {
                (1, 3_600_000_000_000)
            } else {
                return Err(format!("unknown unit in duration {orig:?}"));
            };
            rest = &rest[unit_len..];

            let int_nanos = if int.is_empty() {
                0
            } else {
                int.parse::<u128>()
                    .ok()
                    .and_then(|v| v.checked_mul(unit))
                    .ok_or_else(overflow)?
            };
            let frac = &frac[..frac.len().min(20)];
            let frac_nanos = if frac.is_empty() {
                0
            } else {
                frac.parse::<u128>().map_err(|_| invalid())? * unit / 10u128.pow(frac.len() as u32)
            };
            total = total
                .checked_add(int_nanos + frac_nanos)
                .ok_or_else(overflow)?;
        }
        let secs = u64::try_from(total / 1_000_000_000).map_err(|_| overflow())?;
        Ok(Duration(StdDuration::new(
            secs,
            (total % 1_000_000_000) as u32,
        )))
    }
}

impl fmt::Display for Duration {
    /// Formats like Go's `time.Duration.String()`: `"30s"`, `"1h30m0s"`,
    /// `"1.5s"`, `"500ms"`, `"0s"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.0;
        if d == StdDuration::ZERO {
            return f.write_str("0s");
        }
        let nanos = d.as_nanos();
        if nanos < 1_000 {
            return write!(f, "{nanos}ns");
        }
        if nanos < 1_000_000 {
            return write_frac(f, nanos, 1_000, "\u{b5}s");
        }
        if nanos < 1_000_000_000 {
            return write_frac(f, nanos, 1_000_000, "ms");
        }
        let secs = d.as_secs();
        let sub = d.subsec_nanos();
        let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        if h > 0 {
            write!(f, "{h}h{m}m")?;
            return write_secs(f, s, sub);
        }
        if m > 0 {
            write!(f, "{m}m")?;
            return write_secs(f, s, sub);
        }
        write_secs(f, s, sub)
    }
}

fn write_secs(f: &mut fmt::Formatter<'_>, s: u64, sub_nanos: u32) -> fmt::Result {
    if sub_nanos == 0 {
        write!(f, "{s}s")
    } else {
        let frac = format!("{sub_nanos:09}");
        let frac = frac.trim_end_matches('0');
        write!(f, "{s}.{frac}s")
    }
}

fn write_frac(f: &mut fmt::Formatter<'_>, nanos: u128, unit: u128, name: &str) -> fmt::Result {
    let whole = nanos / unit;
    let rem = nanos % unit;
    if rem == 0 {
        write!(f, "{whole}{name}")
    } else {
        let width = (unit as f64).log10() as usize;
        let frac = format!("{rem:0width$}");
        let frac = frac.trim_end_matches('0');
        write!(f, "{whole}.{frac}{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_round_trip() {
        for (s, disp) in [
            ("30s", "30s"),
            ("1h30m", "1h30m0s"),
            ("500ms", "500ms"),
            ("1.5s", "1.5s"),
            ("2h", "2h0m0s"),
            ("90m", "1h30m0s"),
            ("0", "0s"),
        ] {
            let d: Duration = s.parse().unwrap();
            assert_eq!(d.to_string(), disp, "input {s}");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!("".parse::<Duration>().is_err());
        assert!("5".parse::<Duration>().is_err());
        assert!("5x".parse::<Duration>().is_err());
        assert!("-5s".parse::<Duration>().is_err());
        assert!(".s".parse::<Duration>().is_err());
        assert!("1.2.3s".parse::<Duration>().is_err());
    }

    #[test]
    fn too_large_is_an_error_not_a_panic() {
        assert!("99999999999999999999h".parse::<Duration>().is_err());
        assert!("99999999999999999999999999999999999999999h"
            .parse::<Duration>()
            .is_err());
    }

    #[test]
    fn exact_nanoseconds() {
        let d: Duration = "2562047h47m16.854775807s".parse().unwrap();
        assert_eq!(d.0, StdDuration::new(9_223_372_036, 854_775_807));
        let d: Duration = ".5s".parse().unwrap();
        assert_eq!(d.0, StdDuration::from_millis(500));
        let d: Duration = "1.5h".parse().unwrap();
        assert_eq!(d.0, StdDuration::from_secs(5400));
    }
}
