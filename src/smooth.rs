pub const TAU: f64 = 0.08;
pub const MIN_SPEED: f64 = 300.0;
pub const STREAK_WINDOW: f64 = 0.08;
pub const ACCEL: f64 = 0.15;
pub const MAX_GAIN: f64 = 3.0;

pub struct Axis {
    remaining: f64,
    carry: f64,
    last: f64,
    gain: f64,
}

impl Axis {
    pub const fn new() -> Self {
        Axis {
            remaining: 0.0,
            carry: 0.0,
            last: f64::NEG_INFINITY,
            gain: 1.0,
        }
    }

    pub fn push(&mut self, delta: i32, now: f64) {
        let d = delta as f64;
        if self.remaining * d < 0.0 {
            self.stop();
        } else if now - self.last < STREAK_WINDOW {
            self.gain = (self.gain + ACCEL).min(MAX_GAIN);
        } else {
            self.gain = 1.0;
        }
        self.last = now;
        self.remaining += d * self.gain;
    }

    pub fn stop(&mut self) {
        self.remaining = 0.0;
        self.carry = 0.0;
        self.gain = 1.0;
    }

    pub fn active(&self) -> bool {
        self.remaining != 0.0
    }

    pub fn tick(&mut self, dt: f64) -> i32 {
        let mut step = self.remaining * (1.0 - (-dt / TAU).exp());
        let min = MIN_SPEED * dt;
        if step.abs() < min {
            step = min.copysign(self.remaining);
        }
        if step.abs() >= self.remaining.abs() {
            step = self.remaining;
        }
        self.remaining -= step;
        self.carry += step;
        let out = if self.remaining == 0.0 {
            self.carry.round()
        } else {
            self.carry.trunc()
        };
        self.carry -= out;
        if self.remaining == 0.0 {
            self.carry = 0.0;
        }
        out as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f64 = 1.0 / 60.0;

    fn drain(a: &mut Axis) -> i32 {
        let mut sum = 0;
        for _ in 0..10_000 {
            sum += a.tick(FRAME);
            if !a.active() {
                return sum;
            }
        }
        panic!("动画没有结束");
    }

    #[test]
    fn slow_notches_emit_exact_total() {
        let mut a = Axis::new();
        a.push(120, 0.0);
        assert_eq!(drain(&mut a), 120);
        a.push(-120, 1.0);
        assert_eq!(drain(&mut a), -120);
    }

    #[test]
    fn first_frame_is_partial_not_all_at_once() {
        let mut a = Axis::new();
        a.push(120, 0.0);
        let first = a.tick(FRAME);
        assert!(first > 0 && first < 120, "first = {first}");
    }

    #[test]
    fn single_notch_settles_quickly() {
        let mut a = Axis::new();
        a.push(120, 0.0);
        let mut frames = 0;
        while a.active() {
            a.tick(FRAME);
            frames += 1;
        }
        assert!(frames as f64 * FRAME < 0.3, "took {frames} frames");
    }

    #[test]
    fn fast_notches_accelerate() {
        let mut a = Axis::new();
        for i in 0..5 {
            a.push(120, i as f64 * 0.02);
        }
        assert!(drain(&mut a) > 600);
    }

    #[test]
    fn reversing_drops_leftover() {
        let mut a = Axis::new();
        a.push(120, 0.0);
        a.tick(FRAME);
        a.push(-120, 0.01);
        assert_eq!(drain(&mut a), -120);
    }
}
