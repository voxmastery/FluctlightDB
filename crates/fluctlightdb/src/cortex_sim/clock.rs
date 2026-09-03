//! Logical discrete-event clock for deterministic simulation.

#[derive(Debug, Clone, Copy, Default)]
pub struct CortexClock {
    micros: u64,
}

impl CortexClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now_micros(&self) -> u64 {
        self.micros
    }

    pub fn advance(&mut self, delta_micros: u64) {
        self.micros = self.micros.saturating_add(delta_micros);
    }

    pub fn set(&mut self, micros: u64) {
        self.micros = micros;
    }
}
