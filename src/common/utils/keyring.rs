use keyring::Entry;

use crate::{here, Error, ErrorLocation, Location};

#[must_use]
pub struct Keyring {
    entry: Entry,
}

impl Keyring {
    pub fn new<T, E>(app_name: T, username: E) -> Self
    where
        T: AsRef<str>,
        E: AsRef<str>,
    {
        let service = format!("novel-{}", app_name.as_ref());
        let entry = Entry::new(&service, username.as_ref());

        Self { entry }
    }

    pub fn get_password(&self) -> Result<String, Error> {
        Ok(self.entry.get_password()?)
    }

    pub fn set_password<T>(&self, password: T) -> Result<(), Error>
    where
        T: AsRef<str>,
    {
        Ok(self
            .entry
            .set_password(password.as_ref())
            .location(here!())?)
    }

    pub fn delete_password(&self) -> Result<(), Error> {
        Ok(self.entry.delete_password()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    #[cfg_attr(feature = "ci", ignore)]
    fn keyring() -> Result<(), Error> {
        let password = "test-username";
        let keyring = Keyring::new("test", password);

        keyring.set_password(password)?;
        assert_eq!(keyring.get_password()?, password);

        keyring.delete_password()?;

        Ok(())
    }
}