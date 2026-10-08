use std::fmt;
use std::str::FromStr;

macro_rules! complex_type {
    ($name:ident, $float:ty, $doc:literal) => {
        #[doc = $doc]
        ///
        /// Parses the same text as Go's `strconv.ParseComplex`: `3`, `2i`,
        /// `1+2i`, `(1-2.5i)`, `inf+nani`, and hexadecimal parts such as
        /// `0x1p-2i`. A `j` suffix is an error. Displays like Go, as
        /// `(1+2i)`.
        #[derive(Debug, Default, Clone, Copy, PartialEq)]
        pub struct $name {
            /// The real part.
            pub re: $float,
            /// The imaginary part.
            pub im: $float,
        }

        impl $name {
            /// A complex number from its real and imaginary parts.
            pub fn new(re: $float, im: $float) -> Self {
                Self { re, im }
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, String> {
                let (re, im) = parse_complex::<$float>(s)?;
                Ok(Self { re, im })
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let im = format_g(self.im as f64, <$float as Float>::DIGITS);
                let sign = if im.starts_with(['+', '-']) { "" } else { "+" };
                write!(
                    f,
                    "({}{sign}{im}i)",
                    format_g(self.re as f64, <$float as Float>::DIGITS)
                )
            }
        }

        #[cfg(feature = "num-complex")]
        impl From<$name> for num_complex::Complex<$float> {
            fn from(c: $name) -> Self {
                num_complex::Complex::new(c.re, c.im)
            }
        }

        #[cfg(feature = "num-complex")]
        impl From<num_complex::Complex<$float>> for $name {
            fn from(c: num_complex::Complex<$float>) -> Self {
                Self { re: c.re, im: c.im }
            }
        }
    };
}

complex_type!(
    Complex128,
    f64,
    "A complex number with `f64` parts, like Go's `complex128`."
);
complex_type!(
    Complex64,
    f32,
    "A complex number with `f32` parts, like Go's `complex64`."
);

trait Float: Copy + FromStr {
    const DIGITS: u32;
    const ZERO: Self;
    fn from_f64(v: f64) -> Option<Self>;
    fn is_finite(self) -> bool;
}

impl Float for f64 {
    const DIGITS: u32 = 17;
    const ZERO: Self = 0.0;
    fn from_f64(v: f64) -> Option<Self> {
        Some(v)
    }
    fn is_finite(self) -> bool {
        f64::is_finite(self)
    }
}

impl Float for f32 {
    const DIGITS: u32 = 9;
    const ZERO: Self = 0.0;
    fn from_f64(v: f64) -> Option<Self> {
        let f = v as f32;
        (f.is_finite() || !v.is_finite()).then_some(f)
    }
    fn is_finite(self) -> bool {
        f32::is_finite(self)
    }
}

fn parse_complex<F: Float>(orig: &str) -> Result<(F, F), String> {
    let syntax = || format!("invalid complex number {orig:?}");
    let mut s = orig;
    if s.len() >= 2 && s.starts_with('(') && s.ends_with(')') {
        s = &s[1..s.len() - 1];
    }
    let (re, n) = float_prefix::<F>(s, orig)?;
    s = &s[n..];
    if s.is_empty() {
        return Ok((re, F::ZERO));
    }
    match s.as_bytes()[0] {
        b'+' => {
            if s.len() > 1 && s.as_bytes()[1] != b'+' {
                s = &s[1..];
            }
        }
        b'-' => {}
        b'i' if s.len() == 1 => return Ok((F::ZERO, re)),
        _ => return Err(syntax()),
    }
    let (im, n) = float_prefix::<F>(s, orig)?;
    if &s[n..] != "i" {
        return Err(syntax());
    }
    Ok((re, im))
}

fn float_prefix<F: Float>(s: &str, orig: &str) -> Result<(F, usize), String> {
    let syntax = || format!("invalid complex number {orig:?}");
    if let Some((v, n)) = special(s) {
        return Ok((F::from_f64(v).ok_or_else(syntax)?, n));
    }
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let hex = b.len() >= i + 2 && b[i] == b'0' && (b[i + 1] | 0x20) == b'x';
    if hex {
        i += 2;
    }
    let is_digit = |c: u8| c.is_ascii_digit() || (hex && c.is_ascii_hexdigit());
    let mut digits = 0;
    let mut seen_dot = false;
    while i < b.len() {
        match b[i] {
            b'_' => {}
            b'.' if !seen_dot => seen_dot = true,
            c if is_digit(c) => digits += 1,
            _ => break,
        }
        i += 1;
    }
    if digits == 0 {
        return Err(syntax());
    }
    let exp_char = if hex { b'p' } else { b'e' };
    if i < b.len() && (b[i] | 0x20) == exp_char {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
            i += 1;
        }
        if !b[start..i].iter().any(u8::is_ascii_digit) {
            return Err(syntax());
        }
    } else if hex {
        return Err(syntax());
    }
    let text = &s[..i];
    if text.contains('_') && !underscore_ok(text) {
        return Err(syntax());
    }
    let clean = text.replace('_', "");
    let v = if hex {
        F::from_f64(parse_hex(&clean)).ok_or_else(syntax)?
    } else {
        clean.parse::<F>().map_err(|_| syntax())?
    };
    if !v.is_finite() {
        return Err(format!("complex number {orig:?} is out of range"));
    }
    Ok((v, i))
}

fn special(s: &str) -> Option<(f64, usize)> {
    let lower = s.to_ascii_lowercase();
    let (sign, nsign, rest) = match lower.as_bytes().first()? {
        b'+' => (1.0, 1, &lower[1..]),
        b'-' => (-1.0, 1, &lower[1..]),
        _ => (1.0, 0, lower.as_str()),
    };
    if rest.starts_with("infinity") {
        return Some((f64::INFINITY * sign, nsign + 8));
    }
    if rest.starts_with("inf") {
        return Some((f64::INFINITY * sign, nsign + 3));
    }
    if nsign == 0 && rest.starts_with("nan") {
        return Some((f64::NAN, 3));
    }
    None
}

fn underscore_ok(s: &str) -> bool {
    let b = s.trim_start_matches(['+', '-']).as_bytes();
    let mut saw = b'^';
    let mut i = 0;
    let hex = b.len() >= 2 && b[0] == b'0' && (b[1] | 0x20) == b'x';
    if b.len() >= 2 && b[0] == b'0' && matches!(b[1] | 0x20, b'b' | b'o' | b'x') {
        i = 2;
        saw = b'0';
    }
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_digit() || (hex && c.is_ascii_hexdigit()) {
            saw = b'0';
        } else if c == b'_' {
            if saw != b'0' {
                return false;
            }
            saw = b'_';
        } else {
            if saw == b'_' {
                return false;
            }
            saw = b'!';
        }
        i += 1;
    }
    saw != b'_'
}

fn parse_hex(s: &str) -> f64 {
    let (neg, s) = match s.as_bytes()[0] {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    let s = &s[2..];
    let (mantissa, exp) = s.split_once(['p', 'P']).unwrap();
    let mut value = 0f64;
    let mut scale = exp.parse::<i32>().unwrap_or(if exp.starts_with('-') {
        i32::MIN / 2
    } else {
        i32::MAX / 2
    });
    let mut after_dot = false;
    for c in mantissa.chars() {
        if c == '.' {
            after_dot = true;
            continue;
        }
        value = value * 16.0 + c.to_digit(16).unwrap() as f64;
        if after_dot {
            scale = scale.saturating_sub(4);
        }
    }
    let mut v = value;
    let mut scale = scale.clamp(-2200, 2200);
    while scale > 1000 {
        v *= 2f64.powi(1000);
        scale -= 1000;
    }
    while scale < -1000 {
        v *= 2f64.powi(-1000);
        scale += 1000;
    }
    v *= 2f64.powi(scale);
    if neg {
        -v
    } else {
        v
    }
}

/// Format like Go's `strconv.FormatFloat(v, 'g', -1, bits)`.
pub(crate) fn format_g(v: f64, digits: u32) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "+Inf".into()
        } else {
            "-Inf".into()
        };
    }
    let shortest = if digits == 9 {
        format!("{:e}", v as f32)
    } else {
        format!("{v:e}")
    };
    let (mantissa, exp) = shortest.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exp.abs());
    }
    if digits == 9 {
        (v as f32).to_string()
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_like_go() {
        for (s, re, im) in [
            ("0", 0.0, 0.0),
            ("3", 3.0, 0.0),
            ("2i", 0.0, 2.0),
            ("+1i", 0.0, 1.0),
            ("-1i", 0.0, -1.0),
            ("1+2i", 1.0, 2.0),
            ("1-2i", 1.0, -2.0),
            ("(1+2i)", 1.0, 2.0),
            ("-3.5+0.25i", -3.5, 0.25),
            ("1e3+1e-3i", 1000.0, 0.001),
            ("1e+2i", 0.0, 100.0),
            (".5+1.i", 0.5, 1.0),
            ("1_000+2_0i", 1000.0, 20.0),
            ("0x10p0+0x1p-2i", 16.0, 0.25),
            ("0x1.8p1", 3.0, 0.0),
            ("-0x1p-1074", -5e-324, 0.0),
        ] {
            let c: Complex128 = s.parse().unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!((c.re, c.im), (re, im), "{s}");
        }
    }

    #[test]
    fn specials() {
        let c: Complex128 = "infi".parse().unwrap();
        assert_eq!((c.re, c.im), (0.0, f64::INFINITY));
        let c: Complex128 = "-inf-Infinityi".parse().unwrap();
        assert_eq!((c.re, c.im), (f64::NEG_INFINITY, f64::NEG_INFINITY));
        let c: Complex128 = "NaN+NaNi".parse().unwrap();
        assert!(c.re.is_nan() && c.im.is_nan());
        let c: Complex128 = "1+infi".parse().unwrap();
        assert_eq!(c.im, f64::INFINITY);
    }

    #[test]
    fn rejects_like_go() {
        for s in [
            "", "i", "j", "1+2j", "2j", "1++2i", "(1+2i", "1+2i)", "3+", "1+2", "2i3", "1e",
            "1e+i", "+NaN", "+NaNi", "0x10", "1__0", "_1", "1_", "1 + 2i", "1e400", "1e400i",
        ] {
            assert!(s.parse::<Complex128>().is_err(), "{s:?} should not parse");
        }
        assert!("1e39".parse::<Complex64>().is_err());
        assert!("1e39".parse::<Complex128>().is_ok());
    }

    #[test]
    fn displays_like_go() {
        for (c, want) in [
            (Complex128::new(1.0, 2.0), "(1+2i)"),
            (Complex128::new(1.5, -0.25), "(1.5-0.25i)"),
            (Complex128::new(0.0, 0.0), "(0+0i)"),
            (Complex128::new(1e6, 1e21), "(1e+06+1e+21i)"),
            (
                Complex128::new(999999.0, 1234567.0),
                "(999999+1.234567e+06i)",
            ),
            (Complex128::new(123456.0, 0.0001), "(123456+0.0001i)"),
            (Complex128::new(1e-5, f64::NAN), "(1e-05+NaNi)"),
            (
                Complex128::new(f64::INFINITY, f64::NEG_INFINITY),
                "(+Inf-Infi)",
            ),
        ] {
            assert_eq!(c.to_string(), want);
            if !want.contains("NaN") {
                assert_eq!(want.parse::<Complex128>().unwrap(), c);
            }
        }
        assert_eq!(Complex64::new(0.1, 0.2).to_string(), "(0.1+0.2i)");
    }
}
