//! Lecture / écriture d'entiers gros-boutistes (format des FRM, PRO et MAP de Fallout).

use crate::error::{Error, Result};

/// Curseur de lecture sur un tampon ; toute lecture hors limites renvoie [`Error::Truncated`].
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let s = &self.data[self.pos..end];
                self.pos = end;
                Ok(s)
            }
            None => Err(Error::Truncated {
                what: "lecture",
                at: self.pos,
            }),
        }
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    /// Mot gros-boutiste situé `words` mots plus loin, sans avancer.
    pub fn peek_i32(&self, words: usize) -> Result<i32> {
        let at = self.pos + words * 4;
        self.data
            .get(at..at + 4)
            .map(|b| i32::from_be_bytes(b.try_into().unwrap()))
            .ok_or(Error::Truncated {
                what: "aperçu", at
            })
    }
}

/// Écriture gros-boutiste dans un `Vec<u8>`.
pub trait PutBe {
    fn put_u16(&mut self, v: u16);
    fn put_i16(&mut self, v: i16);
    fn put_u32(&mut self, v: u32);
    fn put_i32(&mut self, v: i32);
}

impl PutBe for Vec<u8> {
    fn put_u16(&mut self, v: u16) {
        self.extend_from_slice(&v.to_be_bytes());
    }
    fn put_i16(&mut self, v: i16) {
        self.extend_from_slice(&v.to_be_bytes());
    }
    fn put_u32(&mut self, v: u32) {
        self.extend_from_slice(&v.to_be_bytes());
    }
    fn put_i32(&mut self, v: i32) {
        self.extend_from_slice(&v.to_be_bytes());
    }
}
