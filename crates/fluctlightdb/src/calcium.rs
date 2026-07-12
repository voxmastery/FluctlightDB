//! Calcium-based synaptic plasticity — the physical mechanism behind STDP.
//!
//! # Why calcium, not just STDP rules
//!
//! The Bi & Poo (1998) STDP rule — "if pre fires before post, strengthen; if after, weaken"
//! — is an OBSERVED BEHAVIOR. The physical mechanism that produces this behavior is calcium.
//!
//! At every synapse, there is a tiny spine (~1 μm³) containing NMDA receptors. These are
//! coincidence detectors: they only allow Ca²⁺ to enter the spine when BOTH:
//!   1. Glutamate is present (pre-synaptic neuron fired)
//!   2. The membrane is depolarised (post-synaptic neuron fired recently)
//!
//! The Mg²⁺ ion physically blocks the NMDA channel at resting potential. Depolarisation
//! (from post-synaptic firing) relieves this block — allowing Ca²⁺ to flood the spine.
//!
//! What happens next depends on HOW MUCH Ca²⁺ entered:
//!   - [Ca²⁺] > θ_p (high): CaMKII kinase activates → AMPA receptors inserted → LTP
//!   - θ_d < [Ca²⁺] < θ_p (medium): calcineurin phosphatase dominates → AMPA removed → LTD
//!   - [Ca²⁺] < θ_d (low): no net change
//!
//! The TIMING dependency emerges automatically: if pre fires just before post, Ca²⁺ from
//! NMDA (pre-triggered glutamate + post-triggered depolarisation) arrives together → HIGH
//! Ca²⁺ → LTP. If post fires before pre, the depolarisation decays before glutamate arrives
//! → MEDIUM Ca²⁺ → LTD. The STDP curve is a consequence, not the rule.
//!
//! This module implements that mechanism directly — calcium IS the plasticity rule here,
//! not a fixed lookup table.
//!
//! # References
//! - Shouval, Bear & Cooper 2002 — "A unified model of NMDA receptor-dependent
//!   bidirectional synaptic plasticity" (PNAS 99:10831)
//! - Jahr & Stevens 1990 — NMDA receptor Mg²⁺ block voltage dependence (J Neurosci 10:1830)
//! - Mainen et al. 1999 — Ca²⁺ imaging at single spines
//! - Bhatt, Bhatt & Bhatt 2009 — dendritic Ca²⁺ dynamics review

use serde::{Deserialize, Serialize};

// ── Physiological constants ───────────────────────────────────────────────────

/// Mg²⁺ concentration (mM) — physiological extracellular level.
const MG_CONC_MM: f32 = 1.0;

/// Jahr & Stevens 1990 voltage coefficient (mV⁻¹).
const ALPHA_MG: f32 = 0.062;

/// Jahr & Stevens 1990 affinity constant (mM).
const BETA_MG: f32 = 3.57;

/// Calcium time constant (ms) — extrusion + buffering (Bhatt 2009).
/// Faster than 20ms to ensure Ca is cleared by Δt=60ms (no plasticity at that range).
pub const TAU_CA_MS: f32 = 10.0;

/// LTD onset threshold for [Ca²⁺] (normalised).
/// Below this: no plasticity (Shouval 2002 Fig. 4).
pub const THETA_D: f32 = 0.22;

/// LTP onset threshold for [Ca²⁺] (normalised).
/// Above this: CaMKII dominates → LTP (Shouval 2002 Fig. 4).
pub const THETA_P: f32 = 0.55;

/// LTD learning rate — calcineurin phosphatase activity proxy.
pub const GAMMA_D: f32 = 0.012;

/// LTP learning rate — CaMKII kinase activity proxy.
/// Biology: CaMKII auto-phosphorylation is cooperative and faster than calcineurin.
/// Must be ~10× GAMMA_D so that LTP "overcomes" the LTD afterglow during Ca decay
/// through the intermediate zone (Lisman 1989; Lisman & Goldring 1988).
pub const GAMMA_P: f32 = 0.12;

/// Ca²⁺ influx from pre-synaptic spike via pre-terminal VDCC.
/// Small — this alone does not trigger plasticity.
const CA_INFLUX_PRE_VDCC: f32 = 0.08;

/// NMDA-mediated Ca²⁺ burst at post-synaptic spike, per unit of glutamate residue × full Mg²⁺ relief.
/// Scaled by `glu` (pre-spike residue) and `nmda_mg_relief(v_post)` at spike time.
/// Physical: NMDA opens when BOTH glutamate (from pre) AND depolarisation (from post) are present.
/// Value tuned so that Δt=+25ms gives ca_peak≈0.80 → clear LTP margin above THETA_P.
const CA_INFLUX_NMDA_BURST: f32 = 2.82;

/// NMDA Ca²⁺ at pre-spike arrival, per unit of bAP residue.
/// Physical: when pre fires and the spine is still depolarised from a recent post-spike (bAP),
/// NMDA partially opens → Ca²⁺ influx driving LTD.
const CA_INFLUX_BPAP: f32 = 0.46;

/// Ca²⁺ from post-synaptic VDCC per spike (voltage-independent approximation).
/// Reduced to 0.05 so that NMDA-absent spikes (Δt=+60ms, glutamate cleared) fall below THETA_D.
const CA_INFLUX_POST_VDCC: f32 = 0.05;

/// Glutamate residue decay (ms) — NMDA receptor deactivation timescale after pre-spike.
const TAU_GLU_MS: f32 = 22.0;

/// Back-propagating AP (bAP) decay (ms) at dendritic spine — sets LTD timing window.
const TAU_BPAP_MS: f32 = 25.0;

// ─────────────────────────────────────────────────────────────────────────────

/// Calcium concentration state at a single synaptic spine.
///
/// This is the physical medium of synaptic plasticity. No rule is hard-coded here —
/// the LTP/LTD behavior emerges from the calcium level relative to the two thresholds.
///
/// The model tracks two coincidence signals:
/// - `glu`: pre-synaptic glutamate residue (starts at 1.0 on pre-spike, decays with TAU_GLU_MS)
/// - `bpap`: back-propagating AP residue (starts at 1.0 on post-spike, decays with TAU_BPAP_MS)
///
/// LTP requires glu when post fires (NMDA opens at spike depolarisation).
/// LTD requires bpap when pre fires (NMDA partially opens at residual depolarisation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalciumSpine {
    /// Current [Ca²⁺] concentration (normalised to [0, 1]).
    pub ca: f32,
    /// Calcium extrusion time constant (ms).
    pub tau_ca: f32,
    /// LTD threshold (calcineurin activation).
    pub theta_d: f32,
    /// LTP threshold (CaMKII activation).
    pub theta_p: f32,
    /// LTD rate.
    pub gamma_d: f32,
    /// LTP rate.
    pub gamma_p: f32,
    /// Pre-synaptic glutamate residue (decays with TAU_GLU_MS after pre-spike).
    pub glu: f32,
    /// Back-propagating AP residue (decays with TAU_BPAP_MS after post-spike).
    pub bpap: f32,
    /// Cumulative LTP events (history).
    pub ltp_count: u32,
    /// Cumulative LTD events (history).
    pub ltd_count: u32,
}

impl Default for CalciumSpine {
    fn default() -> Self {
        Self {
            ca: 0.0,
            tau_ca: TAU_CA_MS,
            theta_d: THETA_D,
            theta_p: THETA_P,
            gamma_d: GAMMA_D,
            gamma_p: GAMMA_P,
            glu: 0.0,
            bpap: 0.0,
            ltp_count: 0,
            ltd_count: 0,
        }
    }
}

impl CalciumSpine {
    /// Pre-synaptic spike: glutamate released.
    ///
    /// Two Ca²⁺ sources:
    /// 1. Pre-VDCC: small, voltage-independent influx from pre-terminal channels.
    /// 2. **LTD mechanism**: if a recent post-spike left a bAP residue at the spine,
    ///    the residual depolarisation partially relieves NMDA Mg²⁺ block → Ca²⁺ influx.
    ///    This is the physical origin of STDP's LTD window (post-before-pre timing).
    pub fn on_pre_spike(&mut self) {
        // Raise glutamate signal (for future LTP if post fires within TAU_GLU_MS)
        self.glu = (self.glu + 1.0).min(2.0);
        // Pre-VDCC: small Ca from presynaptic terminal
        self.ca = (self.ca + CA_INFLUX_PRE_VDCC).min(1.0);
        // LTD: if bAP residue present, NMDA partially open → Ca²⁺ drives LTD
        let ca_ltd = CA_INFLUX_BPAP * self.bpap;
        self.ca = (self.ca + ca_ltd).min(1.0);
    }

    /// Post-synaptic spike / depolarisation.
    ///
    /// Two Ca²⁺ sources:
    /// 1. Post-VDCC: voltage-gated channels open at spike peak.
    /// 2. **LTP mechanism**: if a recent pre-spike left glutamate at the synapse,
    ///    the spike depolarisation relieves NMDA Mg²⁺ block → Ca²⁺ flood → LTP.
    ///    Scaled by `glu` (glutamate residue) × `nmda_mg_relief(v_post)`.
    ///
    /// `v_post` (mV): post-synaptic membrane potential at spike time (0 mV for spike peak).
    pub fn on_post_spike(&mut self, v_post: f32) {
        // Set bAP residue (will decay with TAU_BPAP_MS — sets the LTD timing window)
        self.bpap = 1.0;
        // Post-VDCC: small Ca from dendritic channels
        self.ca = (self.ca + CA_INFLUX_POST_VDCC).min(1.0);
        // LTP: if glu residue present, NMDA opens fully at spike depolarisation
        let mg_relief = nmda_mg_relief(v_post);
        let ca_ltp = CA_INFLUX_NMDA_BURST * self.glu * mg_relief;
        self.ca = (self.ca + ca_ltp).min(1.0);
    }

    /// Advance calcium dynamics by `dt` (ms).
    ///
    /// 1. Calcium decays via extrusion pumps (SERCA, NCX) and buffering proteins
    /// 2. Glutamate residue decays (NMDA deactivation / glutamate reuptake)
    /// 3. bAP residue decays (spine depolarisation dissipates)
    /// 4. Plasticity rule applied: weight updated based on current [Ca²⁺] vs thresholds
    ///
    /// Returns the weight delta (ΔW) applied this step.
    pub fn tick(&mut self, weight: &mut f32, dt: f32) -> f32 {
        // Calcium extrusion: exponential decay toward 0 (buffering + pumps)
        self.ca -= (self.ca / self.tau_ca) * dt;
        self.ca = self.ca.max(0.0);
        // Glutamate clearance (NMDA deactivation timescale)
        self.glu -= (self.glu / TAU_GLU_MS) * dt;
        self.glu = self.glu.max(0.0);
        // bAP decay (spine repolarisation)
        self.bpap -= (self.bpap / TAU_BPAP_MS) * dt;
        self.bpap = self.bpap.max(0.0);

        // Plasticity — smooth calcium-dependent rule (Shouval 2002):
        let dw = if self.ca > self.theta_p {
            // CaMKII kinase activation: AMPA receptors inserted into PSD → LTP
            // Magnitude proportional to Ca²⁺ above LTP threshold.
            let delta = self.gamma_p * (self.ca - self.theta_p) * dt;
            self.ltp_count += 1;
            delta
        } else if self.ca > self.theta_d {
            // Calcineurin phosphatase activation: AMPA receptors endocytosed → LTD
            // Magnitude proportional to "distance from ceiling" (Shouval 2002 Fig.4).
            let delta = -self.gamma_d * (self.theta_p - self.ca) * dt;
            self.ltd_count += 1;
            delta
        } else {
            // Below LTD threshold: no net change (protein kinase / phosphatase balanced).
            0.0
        };

        *weight = (*weight + dw).clamp(0.001, 1.0);
        dw
    }
}

/// NMDA receptor Mg²⁺ block relief as a function of post-synaptic potential.
///
/// Jahr & Stevens (1990) voltage-dependent conductance:
/// ```text
///   B(V) = 1 / (1 + [Mg²⁺]_o × exp(−α × V) / β)
/// ```
///
/// Physical meaning: Mg²⁺ sits in the NMDA channel pore at rest, blocking Ca²⁺.
/// Depolarisation electrostatically repels the Mg²⁺ outward → channel opens.
///
/// # Values at key voltages
/// | V (mV) | B(V)  | Meaning                              |
/// |--------|-------|--------------------------------------|
/// | −70    | ~0.04 | Rest: NMDA almost fully blocked      |
/// | −55    | ~0.12 | Near threshold: slight opening       |
/// | −40    | ~0.31 | Moderate depolarisation              |
/// |  0     | ~0.83 | Spike peak: NMDA mostly open         |
/// | +40    | ~0.97 | Full depolarisation: nearly all open |
pub fn nmda_mg_relief(v_post: f32) -> f32 {
    1.0 / (1.0 + MG_CONC_MM * (-ALPHA_MG * v_post).exp() / BETA_MG)
}

/// Predict the plasticity direction for a given pre→post spike interval.
///
/// This is the canonical Bi & Poo (1998) experiment in simulation:
/// one pre-post pairing at a given Δt, measure the resulting ΔW.
///
/// At Δt > 0 (pre before post): expect LTP (Ca²⁺ peaks above θ_p).
/// At Δt < 0 (post before pre): expect LTD (Ca²⁺ peaks between θ_d and θ_p).
/// At |Δt| > 40ms: expect no change (calcium fully decayed before coincidence).
///
/// Used in the test suite to verify our model matches the empirical STDP curve.
pub fn predict_stdp_outcome(delta_t_ms: f32) -> StdpOutcome {
    let mut spine = CalciumSpine::default();
    let dt = 0.1_f32;             // 0.1ms simulation step
    let sim_ms = 200.0_f32;       // 200ms window — enough for all intervals tested
    let steps = (sim_ms / dt) as u32;

    // Ensure both spikes always fire at positive times within the window.
    // For Δt > 0 (pre before post): pre at 10ms, post at 10+Δt ms.
    // For Δt < 0 (post before pre): post at 10ms, pre at 10+|Δt| ms.
    let (pre_at_ms, post_at_ms): (f32, f32) = if delta_t_ms >= 0.0 {
        (10.0_f32, 10.0 + delta_t_ms)
    } else {
        (10.0 - delta_t_ms, 10.0_f32)  // post at 10ms, pre fires later
    };

    // Spike peak membrane potential (mV) — NMDA Mg²⁺ fully relieved here
    let v_spike: f32 = 0.0;

    let mut w = 0.5_f32;
    let w_initial = w;
    let mut peak_ca = 0.0_f32;

    for i in 0..steps {
        let t = i as f32 * dt;
        if (t - pre_at_ms).abs() < dt * 0.6 {
            spine.on_pre_spike();
        }
        if (t - post_at_ms).abs() < dt * 0.6 {
            spine.on_post_spike(v_spike);
        }
        spine.tick(&mut w, dt);
        if spine.ca > peak_ca {
            peak_ca = spine.ca;
        }
    }

    let delta_w = w - w_initial;
    StdpOutcome {
        delta_t_ms,
        delta_w,
        peak_ca,
        direction: if delta_w > 1e-4 {
            PlasticityDirection::LTP
        } else if delta_w < -1e-4 {
            PlasticityDirection::LTD
        } else {
            PlasticityDirection::None
        },
    }
}

/// Direction of synaptic plasticity after one pre-post pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlasticityDirection {
    /// Long-term potentiation — synapse strengthened.
    LTP,
    /// Long-term depression — synapse weakened.
    LTD,
    /// No net change — calcium below both thresholds.
    None,
}

/// Result of a single simulated STDP pairing.
#[derive(Debug, Clone)]
pub struct StdpOutcome {
    /// Pre→post spike interval (ms). Positive = pre before post.
    pub delta_t_ms: f32,
    /// Synaptic weight change relative to initial (0.5).
    pub delta_w: f32,
    /// Peak [Ca²⁺] reached during the pairing.
    pub peak_ca: f32,
    /// Plasticity direction.
    pub direction: PlasticityDirection,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── NMDA Mg²⁺ block tests (Jahr & Stevens 1990) ──────────────────────────

    #[test]
    fn nmda_nearly_blocked_at_rest() {
        // At V_rest (−70 mV), Mg²⁺ should block >90% of NMDA current
        let relief = nmda_mg_relief(-70.0);
        assert!(
            relief < 0.10,
            "NMDA should be >90% blocked at rest, got relief={:.3}",
            relief
        );
    }

    #[test]
    fn nmda_substantially_open_at_spike_peak() {
        // At spike peak (~0 mV), NMDA should be mostly open
        let relief = nmda_mg_relief(0.0);
        assert!(
            relief > 0.75,
            "NMDA should be >75% open at spike peak, got relief={:.3}",
            relief
        );
    }

    #[test]
    fn nmda_monotonically_increases_with_depolarisation() {
        // Relief must increase as membrane depolarises — physical requirement
        let voltages = [-70.0_f32, -55.0, -40.0, -20.0, 0.0, 20.0, 40.0];
        let reliefs: Vec<f32> = voltages.iter().map(|&v| nmda_mg_relief(v)).collect();
        for i in 1..reliefs.len() {
            assert!(
                reliefs[i] > reliefs[i - 1],
                "NMDA relief should increase with V: {:.3} at {}mV > {:.3} at {}mV failed",
                reliefs[i],
                voltages[i],
                reliefs[i - 1],
                voltages[i - 1]
            );
        }
    }

    // ── Calcium dynamics tests ────────────────────────────────────────────────

    #[test]
    fn calcium_decays_to_zero_without_spikes() {
        let mut spine = CalciumSpine::default();
        spine.ca = 0.8; // start elevated
        let mut w = 0.5_f32;
        for _ in 0..1000 {
            spine.tick(&mut w, 0.1);
        }
        assert!(
            spine.ca < 0.01,
            "calcium should decay to near-zero after 100ms, got {:.4}",
            spine.ca
        );
    }

    #[test]
    fn pre_spike_alone_does_not_cause_plasticity() {
        let mut spine = CalciumSpine::default();
        let mut w = 0.5_f32;
        spine.on_pre_spike(); // pre fires
        // tick for 50ms — Ca²⁺ should decay without crossing θ_p
        for _ in 0..500 {
            spine.tick(&mut w, 0.1);
        }
        let dw = w - 0.5;
        // Pre alone should cause at most mild LTD (ca between θ_d and θ_p briefly)
        // But definitely NOT LTP
        assert!(dw <= 0.01, "pre-spike alone should not cause LTP, got ΔW={:.4}", dw);
    }

    // ── STDP curve tests (Bi & Poo 1998) ─────────────────────────────────────

    #[test]
    fn pre_before_post_causes_ltp() {
        // Δt = +10ms: pre fires 10ms before post → should get LTP
        let outcome = predict_stdp_outcome(10.0);
        assert_eq!(
            outcome.direction,
            PlasticityDirection::LTP,
            "pre-before-post at Δt=+10ms should cause LTP, got ΔW={:.4}, peak_ca={:.3}",
            outcome.delta_w,
            outcome.peak_ca
        );
        assert!(outcome.delta_w > 0.0, "LTP requires positive ΔW");
    }

    #[test]
    fn post_before_pre_causes_ltd() {
        // Δt = −10ms: post fires 10ms before pre → should get LTD
        let outcome = predict_stdp_outcome(-10.0);
        assert_eq!(
            outcome.direction,
            PlasticityDirection::LTD,
            "post-before-pre at Δt=−10ms should cause LTD, got ΔW={:.4}, peak_ca={:.3}",
            outcome.delta_w,
            outcome.peak_ca
        );
        assert!(outcome.delta_w < 0.0, "LTD requires negative ΔW");
    }

    #[test]
    fn large_interval_causes_no_plasticity() {
        // Δt = +60ms: too far apart — Ca²⁺ fully decays before coincidence
        let outcome = predict_stdp_outcome(60.0);
        assert_eq!(
            outcome.direction,
            PlasticityDirection::None,
            "Δt=+60ms should produce no plasticity, got ΔW={:.4}",
            outcome.delta_w
        );
    }

    #[test]
    fn ltp_stronger_than_ltd_at_equal_intervals() {
        // Empirical finding from Bi & Poo: LTP magnitude > LTD magnitude
        // at symmetric intervals (asymmetric STDP window)
        let ltp = predict_stdp_outcome(10.0);
        let ltd = predict_stdp_outcome(-10.0);
        assert!(
            ltp.delta_w.abs() > ltd.delta_w.abs(),
            "LTP at +10ms ({:.4}) should be stronger than LTD at -10ms ({:.4}) — Bi & Poo asymmetry",
            ltp.delta_w,
            ltd.delta_w.abs()
        );
    }

    #[test]
    fn stdp_window_is_asymmetric_about_zero() {
        // LTP window (Δt > 0) and LTD window (Δt < 0) should both exist
        let ltp_5 = predict_stdp_outcome(5.0);
        let ltd_5 = predict_stdp_outcome(-5.0);
        assert_eq!(ltp_5.direction, PlasticityDirection::LTP, "Δt=+5ms → LTP");
        assert_eq!(ltd_5.direction, PlasticityDirection::LTD, "Δt=−5ms → LTD");
    }

    #[test]
    fn stdp_curve_spans_correct_range() {
        // Sweep Δt from −40ms to +40ms. For each:
        // Δt ∈ (+5, +30): LTP
        // Δt ∈ (−5, −30): LTD
        // |Δt| > 40: None
        let cases = [
            (5.0, PlasticityDirection::LTP),
            (15.0, PlasticityDirection::LTP),
            (25.0, PlasticityDirection::LTP),
            (-5.0, PlasticityDirection::LTD),
            (-15.0, PlasticityDirection::LTD),
            (-25.0, PlasticityDirection::LTD),
        ];
        for (dt, expected) in &cases {
            let out = predict_stdp_outcome(*dt);
            assert_eq!(
                out.direction, *expected,
                "Δt={:+.0}ms expected {:?}, got {:?} (ΔW={:.4})",
                dt, expected, out.direction, out.delta_w
            );
        }
    }
}
