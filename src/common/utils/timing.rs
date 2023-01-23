use std::time::SystemTime;

use crate::{here, Error, ErrorLocation, Location};

#[must_use]
pub struct Timing {
    now: SystemTime,
}

impl Default for Timing {
    fn default() -> Self {
        Self::new()
    }
}

impl Timing {
    pub fn new() -> Self {
        Self {
            now: SystemTime::now(),
        }
    }

    #[inline]
    pub fn elapsed(&mut self) -> Result<String, Error> {
        let result = self.elapsed_str().location(here!())?;
        self.now = SystemTime::now();

        Ok(result)
    }

    #[inline]
    fn elapsed_str(&self) -> Result<String, Error> {
        let time = self.now.elapsed().location(here!())?;

        let mut elapsed = time.as_millis();
        let mut unit = "ms";

        if elapsed <= 1 {
            elapsed = time.as_micros();
            unit = "μs";
        }
        if elapsed <= 1 {
            elapsed = time.as_nanos();
            unit = "ns";
        }

        Ok(format!("{}{}", elapsed, unit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing() -> Result<(), Error> {
        let mut timing = Timing::new();
        let _ = timing.elapsed()?;

        Ok(())
    }
}