//! Allows the reuse of byte buffers.

use std::cell::RefCell;

#[derive(Clone, Debug, Default)]
pub struct Pool {
    // TODO: Use a collection that keeps it sorted by capacity, to allow requests of a certain capacity to return a vec that meets it.
    buffers: RefCell<Vec<Vec<u8>>>,
}

#[derive(Debug)]
pub struct Rented<'a> {
    pool: &'a Pool,
    buffer: Vec<u8>,
}

impl Pool {
    pub fn rent(&self) -> Rented<'_> {
        Rented {
            pool: self,
            buffer: match self.buffers.borrow_mut().pop() {
                Some(mut buffer) => {
                    buffer.clear();
                    buffer
                }
                None => Vec::new(),
            },
        }
    }

    /// Rents a buffer with the specified `capacity`, meaning that it will have space reserved for at least `capacity` bytes.
    pub fn rent_with_capacity(&self, capacity: usize) -> Rented<'_> {
        let mut buffer = self.rent();
        if buffer.capacity() < capacity {
            // The buffer is empty, so reserve will ensure the capacity is at least as many bytes as requested.
            buffer.reserve(capacity);
        }
        buffer
    }

    /// Rents a buffer with the specified `length`, filling the buffer with zeroes.
    pub fn rent_with_length(&self, length: usize) -> Rented<'_> {
        let mut rented = self.rent_with_capacity(length);
        rented.resize(length, 0);
        rented
    }

    pub(crate) fn existing_or_default(pool: Option<&Self>) -> std::borrow::Cow<'_, Self> {
        match pool {
            Some(pool) => std::borrow::Cow::Borrowed(pool),
            None => std::borrow::Cow::Owned(Self::default()),
        }
    }
}

impl Rented<'_> {
    #[inline]
    pub fn to_vec(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    #[inline]
    pub fn as_vec(&self) -> &Vec<u8> {
        &self.buffer
    }

    #[inline]
    pub fn as_vec_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }
}

impl<'a> std::ops::Deref for Rented<'a> {
    type Target = Vec<u8>;

    #[inline]
    fn deref(&self) -> &Vec<u8> {
        self.as_vec()
    }
}

impl<'a> std::ops::DerefMut for Rented<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_vec_mut()
    }
}

impl<'a> Drop for Rented<'a> {
    fn drop(&mut self) {
        self.pool
            .buffers
            .borrow_mut()
            .push(std::mem::take(&mut self.buffer));
    }
}

#[derive(Debug)]
pub enum RentedOrOwned<'a> {
    Rented(Rented<'a>),
    Owned(Vec<u8>),
}

impl<'a> RentedOrOwned<'a> {
    pub fn with_capacity(capacity: usize, pool: Option<&'a Pool>) -> Self {
        match pool {
            None => Self::Owned(Vec::with_capacity(capacity)),
            Some(pool) => Self::Rented(pool.rent_with_capacity(capacity)),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Rented(rented) => rented,
            Self::Owned(owned) => owned,
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            Self::Rented(rented) => rented,
            Self::Owned(ref mut owned) => owned,
        }
    }

    pub fn as_vec(&self) -> &Vec<u8> {
        match self {
            Self::Rented(rented) => &rented.buffer,
            Self::Owned(owned) => owned,
        }
    }

    pub fn as_mut_vec(&mut self) -> &mut Vec<u8> {
        match self {
            Self::Rented(rented) => &mut rented.buffer,
            Self::Owned(owned) => owned,
        }
    }

    pub fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Rented(rented) => rented.clone(),
            Self::Owned(owned) => owned,
        }
    }
}

impl From<Vec<u8>> for RentedOrOwned<'_> {
    fn from(owned: Vec<u8>) -> Self {
        Self::Owned(owned)
    }
}

impl<'a> From<Rented<'a>> for RentedOrOwned<'a> {
    fn from(rented: Rented<'a>) -> Self {
        Self::Rented(rented)
    }
}

impl<'a> std::ops::Deref for RentedOrOwned<'a> {
    type Target = Vec<u8>;

    #[inline]
    fn deref(&self) -> &Vec<u8> {
        self.as_vec()
    }
}

impl<'a> std::ops::DerefMut for RentedOrOwned<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        self.as_mut_vec()
    }
}

#[repr(transparent)]
pub(crate) struct ByteDebug<'a>(&'a [u8]);

impl<'a> From<&'a [u8]> for ByteDebug<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
}

impl<'a, const L: usize> From<&'a [u8; L]> for ByteDebug<'a> {
    fn from(bytes: &'a [u8; L]) -> Self {
        Self(bytes.as_slice())
    }
}

impl<'a> From<&'a Vec<u8>> for ByteDebug<'a> {
    fn from(bytes: &'a Vec<u8>) -> Self {
        Self(bytes.as_slice())
    }
}

impl std::fmt::Debug for ByteDebug<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::fmt::Write as _;

        for (index, value) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_char(' ')?;
            }

            std::fmt::UpperHex::fmt(value, f)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::binary::buffer;

    #[test]
    fn buffer_rent_test() {
        let pool = buffer::Pool::default();

        {
            let mut buffer = pool.rent();
            buffer.push(1);
            buffer.push(2);
        }

        assert_eq!(pool.rent().len(), 0);
        assert!(pool.rent().capacity() > 0);
    }
}
