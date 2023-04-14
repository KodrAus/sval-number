/*!
A format for exchanging arbitrary precision [decimal floating-point](https://en.wikipedia.org/wiki/Decimal_floating_point) numbers.

This library converts text-based numbers like `-123.456e7` into bitstrings like `01010110_10001110_10010010_10100010`.

Specifically, it implements support for an **[IEEE 754](https://en.wikipedia.org/wiki/IEEE_754) compatible
[decimal floating-point](https://en.wikipedia.org/wiki/Decimal_floating_point) bitstring, using
[densely-packed-decimal](https://en.wikipedia.org/wiki/Densely_packed_decimal) encoding, in
[little-endian byte-order](https://en.wikipedia.org/wiki/Endianness)**. Along with the bitstring encoding there is an
equivalent text-based one that can represent all the same values. High-level types that convert between these two formats
and standard Rust numeric types are provided.

# Encoding

The following table demonstrates how various numbers are encoded as 32bit decimals by this library to give you an idea of
how the format works:

| text | binary |
| ----: | ------: |
| _bit layout_ | `tttttttt_tttttttt_ggggtttt_sggggggg` |
| 0 | `00000000_00000000_01010000_00100010` |
| -0 | `00000000_00000000_01010000_10100010` |
| 0e1 | `00000000_00000000_01100000_00100010` |
| 123 | `10100011_00000000_01010000_00100010` |
| -123 | `10100011_00000000_01010000_10100010` |
| 123.456 | `01010110_10001110_00100010_00100010` |
| -123.456 | `01010110_10001110_00100010_10100010` |
| inf | `00000000_00000000_00000000_01111000` |
| -inf | `00000000_00000000_00000000_11111000` |
| nan | `00000000_00000000_00000000_01111100` |
| snan | `00000000_00000000_00000000_01111110` |
| -nan | `00000000_00000000_00000000_11111100` |
| -snan | `00000000_00000000_00000000_11111110` |
| nan(123) | `10100011_00000000_00000000_01111100` |
| snan(123) | `10100011_00000000_00000000_01111110` |

where:

- `s`: The sign bit.
- `g`: The combination field.
- `t`: The trailing significand.

Note that this library _always_ encodes in little-endian byte-order, regardless of the endianness of the underlying platform.

More sizes besides 32bit are supported. The table uses it to minimize space.

# Why decimal bitstrings?

The decimal bitstrings specified in IEEE 754 aren't as widely known as their binary counterparts, but are a good target
for exchanging numbers.

Compared with text, decimal bitstrings are:

- Compact. Instead of encoding 1 digit per byte (8 bits), you get 3 digits per 10 bits.
- Cheap to classify. You can tell from a single byte whether or not a number is positive, negative, whole, infinity, or
NaN. You don't need to reparse the number.

Compared with binary (base-2) bitstrings, decimal bitstrings are:

- Easy to convert between text. You don't need arbitrary-precision arithmetic to encode a human-readable number into a decimal bitstring.
- Precise. You can exactly encode base-10 numbers, which is the base most modern number systems use.
- Consistent. They're a newer standard, so they avoid some ambiguities around NaN payloads and signaling that affect the
portability of binary bitstrings.

# Features and limitations

This library _only_ does conversions between Rust's primitive number types, numbers encoded as text, and decimal bitstrings.
It's not an implementation of decimal arithmetic. It also doesn't do rounding. If a number can't be encoded in a decimal
bitstring of a given width then you'll get `None`s instead of infinities or rounded values.

Decimal numbers in IEEE 754 are non-normalized by-design. The number `1.00` will encode differently to `1` or `1.0`.

This library does support very high precision in no-std, and can work with arbitrary precision when the
`arbitrary-precision` feature is enabled.
*/

#![cfg_attr(not(any(feature = "std", test)), no_std)]

extern crate core;

/*
If you're exploring the source, there are 3 root modules to look at:

- `text`: Implements the text-based format. This module is a no-surprises parser that produces
ranges that cover features of the number, such as the sign, integer digits, decimal point, exponent.
- `binary`: Implements the IEEE 754 compatible decimal floating-point bitstring encoding. This module
is where you'll find the densely-packed-decimal encoding and arbitrary-precision arithmetic for the
exponent. There's some bit-twiddling involved to encode the bits for a number across multiple bytes,
but it's all explained along the way.
- `convert`: Combines the `text` and `binary` modules to convert between strings and Rust primitive
numbers and encoded bitstrings.

There is no special handling for decimal numbers of specific precisions. This is a trade-off between
simplicity and performance. The same implementation handles encoding decimal32 up to decimal256 and beyond.
*/

mod binary;
mod bitstring;
mod convert;
mod error;
mod num;
mod text;

pub use self::{
    bitstring::*,
    error::*,
};

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn bitstr(b: &[u8]) -> String {
        use core::fmt::Write;

        let mut s = String::new();

        for b in b {
            if s.len() > 0 {
                s.write_char('_').expect("infallible string write");
            }

            write!(&mut s, "{:>08b}", b).expect("infallible string write");
        }

        s
    }

    fn nan32(payload: u32) -> f32 {
        f32::from_bits(f32::NAN.to_bits() | (payload & 0x7fffff))
    }

    fn snan32(payload: u32) -> f32 {
        f32::from_bits(nan32(payload).to_bits() & !0x800000)
    }

    fn nan64(payload: u64) -> f64 {
        f64::from_bits(f64::NAN.to_bits() | (payload & 0x7ffffffffffff))
    }

    fn snan64(payload: u64) -> f64 {
        f64::from_bits(nan64(payload).to_bits() & !0x8000000000000)
    }

    #[test]
    fn is_sign_negative() {
        for (f, is_negative) in [
            ("0", false),
            ("-0", true),
            ("123", false),
            ("-123", true),
            ("inf", false),
            ("-inf", true),
            ("nan", false),
            ("-nan", true),
            ("snan", false),
            ("-snan", true),
        ] {
            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            assert_eq!(is_negative, d.is_sign_negative(), "{}", f);
        }
    }

    #[test]
    fn is_finite() {
        for (f, is_finite) in [
            ("0", true),
            ("-0", true),
            ("123", true),
            ("-123", true),
            ("inf", false),
            ("-inf", false),
            ("nan", false),
            ("-nan", false),
            ("snan", false),
            ("-snan", false),
        ] {
            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            assert_eq!(is_finite, d.is_finite(), "{}", f);
        }
    }

    #[test]
    fn is_infinite() {
        for (f, is_infinite) in [
            ("123", false),
            ("-123", false),
            ("inf", true),
            ("-inf", true),
            ("nan", false),
            ("snan", false),
        ] {
            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            assert_eq!(is_infinite, d.is_infinite(), "{}", f);
        }
    }

    #[test]
    fn is_nan() {
        for (f, is_nan) in [
            ("123", false),
            ("-123", false),
            ("inf", false),
            ("-inf", false),
            ("nan", true),
            ("-nan", true),
            ("snan", true),
            ("-snan", true),
        ] {
            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            assert_eq!(is_nan, d.is_nan(), "{}", f);
        }
    }

    #[test]
    fn is_quiet_nan() {
        for (f, is_nan) in [
            ("123", false),
            ("-123", false),
            ("inf", false),
            ("-inf", false),
            ("nan", true),
            ("-nan", true),
            ("snan", false),
            ("-snan", false),
        ] {
            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            assert_eq!(is_nan, d.is_quiet_nan(), "{}", f);
        }
    }

    #[test]
    fn is_signaling_nan() {
        for (f, is_nan) in [
            ("123", false),
            ("-123", false),
            ("inf", false),
            ("-inf", false),
            ("nan", false),
            ("-nan", false),
            ("snan", true),
            ("-snan", true),
        ] {
            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            assert_eq!(is_nan, d.is_signaling_nan(), "{}", f);
        }
    }

    #[test]
    fn decimal_roundtrip_i128() {
        for i in [0i128, 42i128, i128::MIN, i128::MAX] {
            let d = Bitstring::from(i);
            let di = d.try_into().unwrap();

            assert_eq!(i, di);
        }
    }

    #[test]
    fn decimal_roundtrip_u128() {
        for i in [0u128, 42u128, u128::MAX] {
            let d = Bitstring::from_u128(i);
            let di = d.to_u128().unwrap();

            assert_eq!(i, di);
        }
    }

    #[test]
    fn decimal_to_int_with_exponent() {
        for (f, i) in [("17e1", 170i32), ("4e7", 40000000i32), ("170e-1", 17i32)] {
            let d = Bitstring::try_parse_str(f).unwrap();

            assert_eq!(i, d.to_i32().unwrap(), "{}", f);
        }
    }

    #[test]
    fn err_decimal_to_int_exponent_overflow() {
        for f in ["4e618", "17e-1", "1e-1"] {
            let d = Bitstring::try_parse_str(f).unwrap();

            assert!(d.to_i32().is_none(), "{}", f);
        }
    }

    #[test]
    fn decimal_roundtrip_f32() {
        for f in [
            0.0f32,
            17.05e2f32,
            f32::MIN,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let d = Bitstring::from_f32(f);
            let df = d.to_f32().unwrap();

            assert_eq!(f, df);
        }
    }

    #[test]
    fn decimal_roundtrip_f32_nan() {
        for f in [nan32(0), nan32(42), snan32(0), snan32(42)] {
            let d = Bitstring::from_f32(f);
            let df = d.to_f32().unwrap();

            assert_eq!(f.to_bits(), df.to_bits());
        }
    }

    #[test]
    fn err_decimal_to_f32_overflow() {
        let d = Bitstring::try_parse_str("1e106").unwrap();

        assert!(d.to_f32().is_none());
    }

    #[test]
    fn decimal_roundtrip_f64() {
        for f in [
            0.0f64,
            17.05e2f64,
            f64::MIN,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let d = Bitstring::from_f64(f);
            let df = d.to_f64().unwrap();

            assert_eq!(f, df);
        }
    }

    #[test]
    fn decimal_roundtrip_f64_nan() {
        for f in [nan64(0), nan64(42), snan64(0), snan64(42)] {
            let d = Bitstring::from_f64(f);
            let df = d.to_f64().unwrap();

            assert_eq!(f.to_bits(), df.to_bits());
        }
    }

    #[test]
    fn err_decimal_to_f64_overflow() {
        let d = Bitstring::try_parse_str("1e4513").unwrap();

        assert!(d.to_f64().is_none());
    }

    #[test]
    fn decimal_roundtrip_str() {
        for f in [
            "0",
            "-0",
            "0.0",
            "-0.0",
            "435",
            "-435",
            "547473436755",
            "-547473436755",
            "354.55",
            "-354.55",
            "3546.8764256732",
            "-3546.8764256732",
            "0.00012532",
            "-0.00012532",
            "0e1",
            "120e2",
            "-120e2",
            "123e456",
            "-123e456",
            "123e-3",
            "-123e-3",
            "1.2354e-7",
            "-1.2354e-7",
            "1e96",
            "1e-95",
            "1e384",
            "1e-383",
            "1e6144",
            "1e-1643",
            "nan",
            "nan(123)",
            "inf",
            "-inf",
            "snan",
            "snan(123)",
        ] {
            println!("f: {}", f);

            let d = Bitstring::try_parse_str(f).expect("failed to parse decimal");

            // Ensure bitstrings roundtrip through from_le_bytes and to_le_bytes
            assert_eq!(
                bitstr(d.as_le_bytes()),
                bitstr(
                    Bitstring::try_from_le_bytes(d.as_le_bytes())
                        .expect("failed to convert bytes to decimal")
                        .as_le_bytes()
                ),
                "{}",
                f
            );

            let s = d.to_string();

            println!("s: {}", s);

            // Ensure bitstrings roundtrip through to_string and try_parse_str
            assert_eq!(
                bitstr(d.as_le_bytes()),
                bitstr(
                    Bitstring::try_parse_str(&s)
                        .expect("failed to parse decimal")
                        .as_le_bytes()
                ),
                "{} -> {}",
                f,
                s
            );
        }
    }

    #[test]
    fn decimal_size_small_significand_large_exponent() {
        for i in ["1e6100", "1e-6100"] {
            // We only have a single digit, but the exponent is too large for anything smaller than 128bit
            let d = Bitstring::try_parse_str(i).expect("failed to parse decimal");

            assert!(d.as_le_bytes().len() * 8 >= 128, "{}", i);
        }
    }

    #[test]
    fn decimal_size_small_significand() {
        let d = Bitstring::try_parse_str("1").expect("failed to parse decimal");

        assert_eq!(32, d.as_le_bytes().len() * 8);
    }

    #[test]
    fn err_decimal_overflow_exponent() {
        for i in ["1e2147483648", "1e1073741823"] {
            assert!(Bitstring::try_parse_str(i).is_err());
            assert!(Bitstring::try_parse(format_args!("{}", i)).is_err());
        }
    }

    #[test]
    fn err_decimal_overflow_digits() {
        for i in [
            "123456789012345678901234567890123456789012345678901234567890",
            "1234567890123456789.1234567890123456789012345678901234567890",
        ] {
            assert!(Bitstring::try_parse_str(i).is_err());
            assert!(Bitstring::try_parse(format_args!("{}", i)).is_err());
        }
    }

    #[test]
    fn err_decimal_from_invalid_byte_count() {
        let err = Bitstring::try_from_le_bytes(&[]).unwrap_err();
        assert_eq!("the value cannot fit into a decimal of `0` bytes; the width needed is `4` bytes; decimals must be a multiple of 32 bits (4 bytes)", &err.to_string());

        let err = Bitstring::try_from_le_bytes(&[0; 3]).unwrap_err();
        assert_eq!("the value cannot fit into a decimal of `3` bytes; the width needed is `4` bytes; decimals must be a multiple of 32 bits (4 bytes)", &err.to_string());

        let err = Bitstring::try_from_le_bytes(&[0; 32]).unwrap_err();
        assert_eq!(
            "the value cannot fit into a decimal of `20` bytes; the width needed is `32` bytes",
            &err.to_string()
        );
    }

    #[test]
    #[cfg(feature = "arbitrary-precision")]
    fn bigdecimal_large() {
        fn digits(buf: &mut String, n: usize) {
            for i in 0..n {
                buf.push(((i % 9) as u8 + 1 + b'0') as char);
            }
        }

        let mut s = String::new();

        // This is just getting a little silly now. But we want to make sure
        // our real limits are effectively unreachable
        digits(&mut s, 16384);
        s.push('e');
        digits(&mut s, 512);

        let d = BigBitstring::try_parse_str(&s).expect("failed to parse decimal");

        assert_eq!(58272, d.as_le_bytes().len() * 8);
    }
}
