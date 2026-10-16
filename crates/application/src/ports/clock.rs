use chrono::{DateTime, FixedOffset, Utc};

/// Fuente de "ahora". Es un puerto para que los tests no dependan del día en que se ejecutan:
/// en producción se usa [`SystemClock`]; en tests, `testing::FixedClock`.
pub trait Clock: Send + Sync {
    /// El instante actual, en UTC.
    fn now(&self) -> DateTime<Utc>;

    /// Desfase horario del usuario; define qué es "hoy" (ver `ScoringContext::offset`).
    fn offset(&self) -> FixedOffset;
}

/// Reloj real del sistema operativo.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn offset(&self) -> FixedOffset {
        chrono::Local::now().offset().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_the_current_time() {
        let before = Utc::now();
        let now = SystemClock.now();
        let after = Utc::now();

        assert!(before <= now && now <= after);
    }

    #[test]
    fn system_clock_offset_is_a_real_timezone_offset() {
        let seconds = SystemClock.offset().local_minus_utc();

        // Las zonas horarias reales van de UTC-12 a UTC+14.
        assert!((-12 * 3600..=14 * 3600).contains(&seconds));
    }
}
