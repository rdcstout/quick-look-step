use std::collections::BTreeMap;

use memchr::memmem;
use log::warn;

#[cfg(feature = "rayon")]
use rayon::prelude::*;

use crate::{
    ap214::Entity,
    id::Id,
    parse::{parse_entity_decl, parse_entity_fallback},
};

const MAX_RECORDS: usize = 500_000;

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    InvalidDataSection,
    TooManyRecords,
}

#[derive(Debug)]
pub struct StepFile<'a>(pub BTreeMap<usize, Entity<'a>>);
impl<'a> StepFile<'a> {
    /// Parses a STEP file from a raw array of bytes
    /// `data` must be preprocessed by [`strip_flatten`] first
    pub fn parse(data: &'a [u8]) -> Result<Self, ParseError> {
        let blocks = Self::into_blocks(data)?;
        let Some(data_start) = blocks.iter().position(|b| *b == b"DATA;") else {
            return Err(ParseError::InvalidDataSection);
        };
        let data_blocks = &blocks[data_start + 1..];
        let Some(data_end) = data_blocks.iter().position(|b| *b == b"ENDSEC;") else {
            return Err(ParseError::InvalidDataSection);
        };

        // Parse every block, accumulating a Vec of Results.  We parse in
        // single-threaded mode in WASM builds, because there's no thread
        // pool.
        let block_iter = {
            let block_slice = &data_blocks[..data_end];
            #[cfg(feature = "rayon")]
            { block_slice.par_iter() }
            #[cfg(not(feature = "rayon"))]
            { block_slice.iter() }
        };

        let parsed: Vec<(usize, Entity)> = block_iter
            .filter_map(|b| parse_entity_decl(*b)
                .or_else(|e| {
                    warn!("Failed to parse {}: {:?}",
                        std::str::from_utf8(b).unwrap_or("[INVALID UTF-8]"),
                              e);
                    parse_entity_fallback(*b)
                })
                .ok())
            .map(|b| b.1)
            .collect();

        Ok(Self(parsed.into_iter().collect()))
    }

    /// Flattens a STEP file, removing comments and whitespace
    pub fn strip_flatten(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        let mut i = 0;
        let mut quoted = false;
        while i < data.len() {
            match data[i] {
                b'\'' => {
                    quoted = !quoted;
                    out.push(b'\'');
                }
                b'/' if !quoted && data[i..].starts_with(b"/*") => {
                    let Some(comment_end) = memmem::find(&data[i + 2..], b"*/") else {
                        break;
                    };
                    i += comment_end + 4;
                    continue;
                }
                c if !quoted && c.is_ascii_whitespace() => (),
                c => out.push(c),
            }
            i += 1;
        }
        out
    }

    /// Splits a STEP file into individual blocks.  The input must be pre-processed
    /// by [`strip_flatten`] beforehand.
    fn into_blocks(data: &[u8]) -> Result<Vec<&[u8]>, ParseError> {
        let mut blocks = Vec::new();
        let mut start = 0;
        let mut quoted = false;
        for (i, &byte) in data.iter().enumerate() {
            match byte {
                b'\'' => quoted = !quoted,
                b';' if !quoted => {
                    if blocks.len() == MAX_RECORDS {
                        return Err(ParseError::TooManyRecords);
                    }
                    blocks.push(&data[start..=i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        Ok(blocks)
    }

    pub fn entity<T: FromEntity<'a>>(&'a self, i: Id<T>) -> Option<&'a T> {
        self.0.get(&i.0).and_then(T::try_from_entity)
    }
}

impl<'a, T> std::ops::Index<Id<T>> for StepFile<'a> {
    type Output = Entity<'a>;

    fn index(&self, id: Id<T>) -> &Self::Output {
        self.0.get(&id.0).unwrap_or(&Entity::_EmptySlot)
    }
}

pub trait FromEntity<'a> {
    fn try_from_entity(e: &'a Entity<'a>) -> Option<&'a Self>;
}
