extern crate nom;

use anyhow::{Context, Result};
use nom::{
    branch::alt,
    bytes::complete::{tag, take},
    combinator::map,
    multi::{count, many0},
    number::complete::be_u32,
    sequence::tuple,
    IResult,
};

#[derive(Debug)]
pub struct KTestObject {
    name: String,
    num_bytes: u32,
    bytes: Vec<u8>,
}
#[derive(Debug)]
pub struct KTest {
    /// KTest file format version
    version: u32,
    /// KLEE arguments
    args: Vec<String>,
    /// Symbolic arguments
    sym_argvs: u32,
    sym_argv_len: u32,
    num_objects: u32,
    objects: Vec<KTestObject>,
}

/// Parses a .ktest file and returns
pub fn parse_ktest_binary(input: &'static [u8]) -> Result<KTest> {
    let (input, _magic) = magic_number(input).context("Invalid magic number")?;
    let (input, version) = extract_be_u32(input).context("Failed to parse file version")?;
    let (input, args) = extract_arguments(input).context("Failed to extract arguments")?;
    // Version <= 2 does not support symb args
    let (input, (sym_argvs, sym_argv_len)) = if version > 2 {
        extract_sym_args(input).context("Failed to extract symbolic arguments")?
    } else {
        (input, (0, 0))
    };
    let (_input, objects) = extract_objects(input).context("Failed to extract KTest objects")?;

    Ok(KTest {
        version,
        args,
        sym_argvs,
        sym_argv_len,
        num_objects: objects.len() as u32,
        objects,
    })
}

/// Parses the KTest magic number.
fn magic_number(input: &[u8]) -> IResult<&[u8], &[u8]> {
    alt((tag(b"KTEST"), tag(b"BOUT\n")))(input)
}

fn extract_be_u32(input: &[u8]) -> IResult<&[u8], u32> {
    be_u32(input)
}

// Parses the arguments in the KTest file
fn extract_arguments(input: &[u8]) -> IResult<&[u8], Vec<String>> {
    let (input, num) = extract_be_u32(input)?;
    count(extract_argument, num as usize)(input)
}

fn extract_argument(input: &[u8]) -> IResult<&[u8], String> {
    let (input, size) = extract_be_u32(input)?;
    map(take(size), |arg: &[u8]| {
        String::from_utf8(arg.to_owned()).unwrap()
    })(input)
}

fn extract_sym_args(input: &[u8]) -> IResult<&[u8], (u32, u32)> {
    tuple((extract_be_u32, extract_be_u32))(input)
}

fn extract_objects(input: &[u8]) -> IResult<&[u8], Vec<KTestObject>> {
    let (input, _num) = extract_be_u32(input)?;
    many0(extract_object)(input)
}

// Does not work yet
fn extract_object(input: &[u8]) -> IResult<&[u8], KTestObject> {
    let (input, size_name) = extract_be_u32(input)?;
    let (input, name) = map(take(size_name), |name: &[u8]| {
        String::from_utf8(name.to_owned()).unwrap()
    })(input)?;

    let (input, size_bytes) = extract_be_u32(input)?;
    let (input, bytes) = take(size_bytes)(input)?;

    Ok((
        input,
        KTestObject {
            name: name,
            num_bytes: size_bytes,
            bytes: bytes.to_vec(),
        },
    ))
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn invalid_magic_number() {
        let magic = [0, 0, 0, 0, 0];
        let res = magic_number(&magic);
        assert_eq!(res.is_err(), true);
    }

    #[test]
    fn valid_magic_numbers() {
        let ktest = b"KTEST";
        let bout = b"BOUT\n";
        assert_eq!(magic_number(ktest).is_ok(), true);
        assert_eq!(magic_number(bout).is_ok(), true);
    }
}
