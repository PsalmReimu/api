mod dir;
mod keyring;
mod timing;
mod uid;

pub(crate) use self::uid::*;

pub use self::dir::*;
pub use self::keyring::*;
pub use self::timing::*;

// TODO use https://doc.rust-lang.org/std/option/enum.Option.html#method.is_some_and
#[must_use]
#[inline]
pub fn is_some_and<T, F>(option: Option<T>, f: F) -> bool
where
    F: FnOnce(T) -> bool,
{
    match option {
        None => false,
        Some(x) => f(x),
    }
}

#[cfg(test)]
mod tests {
    use crate::Error;

    #[test]
    fn is_some_and() -> Result<(), Error> {
        let x = Some(2);
        assert!(super::is_some_and(x, |x| x > 1));

        let x = Some(0);
        assert!(!super::is_some_and(x, |x| x > 1));

        let x: Option<u32> = None;
        assert!(!super::is_some_and(x, |x| x > 1));

        Ok(())
    }
}
