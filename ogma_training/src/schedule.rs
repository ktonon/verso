use std::f64::consts::PI;

/// Compute learning rate at a given step using cosine schedule with linear warmup.
///
/// - During warmup (step < warmup_steps): linearly ramps from 0 to base_lr
/// - After warmup: cosine decay from base_lr to 0
pub fn cosine_lr(step: usize, warmup_steps: usize, total_steps: usize, base_lr: f64) -> f64 {
    if step < warmup_steps {
        base_lr * step as f64 / warmup_steps.max(1) as f64
    } else {
        let progress = (step - warmup_steps) as f64 / (total_steps - warmup_steps).max(1) as f64;
        base_lr * 0.5 * (1.0 + (PI * progress).cos())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_starts_at_zero() {
        let lr = cosine_lr(0, 100, 1000, 3e-4);
        assert_eq!(lr, 0.0);
    }

    #[test]
    fn warmup_reaches_base_lr() {
        let lr = cosine_lr(100, 100, 1000, 3e-4);
        // At step=warmup_steps, cosine phase starts at progress=0 → cos(0)=1 → lr=base_lr
        assert!((lr - 3e-4).abs() < 1e-10);
    }

    #[test]
    fn warmup_is_linear() {
        let lr = cosine_lr(50, 100, 1000, 3e-4);
        assert!((lr - 1.5e-4).abs() < 1e-10);
    }

    #[test]
    fn cosine_decays_to_zero() {
        let lr = cosine_lr(1000, 100, 1000, 3e-4);
        // At the end: progress=1.0 → cos(pi)=-1 → lr=0
        assert!(lr.abs() < 1e-10);
    }

    #[test]
    fn cosine_midpoint_is_half() {
        let lr = cosine_lr(550, 100, 1000, 3e-4);
        // Midpoint of cosine phase: progress=0.5 → cos(pi/2)=0 → lr=base_lr/2
        assert!((lr - 1.5e-4).abs() < 1e-10);
    }
}
