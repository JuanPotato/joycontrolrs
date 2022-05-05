use std::io::{Error, Result};
use num_traits::{PrimInt, Signed, Zero};

pub fn libc_check_error<T: Signed + PrimInt + Zero>(val: T) -> Result<T> {
    if val < T::zero() {
        Err(Error::last_os_error())
    } else {
        Ok(val)
    }
}
