use std::time::SystemTime;

use crate::Error;

/// Timing tools for performance testing
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
    /// Create a Timing
    pub fn new() -> Self {
        Self {
            now: SystemTime::now(),
        }
    }

    /// Get the time difference from the creation time, and reset the creation time to the current time
    #[inline]
    pub fn elapsed(&mut self) -> Result<String, Error> {
        let result = self.elapsed_str()?;
        self.now = SystemTime::now();

        Ok(result)
    }

    #[inline]
    fn elapsed_str(&self) -> Result<String, Error> {
        let time = self.now.elapsed()?;

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

        Ok(format!("{elapsed}{unit}"))
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
