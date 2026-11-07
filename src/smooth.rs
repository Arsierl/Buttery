//! 平滑滚动的纯计算部分，不依赖 Windows，可以在 Mac 上直接跑测试：
//! rustc --edition 2021 --test src/smooth.rs -o /tmp/smooth_test && /tmp/smooth_test

// 下面几个是手感参数，按实际体验调。
/// 剩余距离每过 TAU 秒衰减到约 37%。越大惯性越长、越"滑"。
pub const TAU: f64 = 0.08;
/// 收尾的最低速度（单位/秒，120 = 一格）。速度降到这里后匀速走完，
/// 不再越走越慢地蹭。越大收尾越干脆。
pub const MIN_SPEED: f64 = 300.0;
/// 两格之间的间隔小于这个值，算作连续快速滚动，开始加速。
pub const STREAK_WINDOW: f64 = 0.08;
/// 连续快速滚动时，每多一格放大多少倍，最多放大到 MAX_GAIN。
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

    /// 收到一格真实滚轮。delta 为 ±120 或高精度滚轮的小值，now 单位为秒。
    pub fn push(&mut self, delta: i32, now: f64) {
        let d = delta as f64;
        if self.remaining * d < 0.0 {
            // 反向滚动：立刻丢掉剩下的惯性
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

    /// 时间前进 dt 秒，返回这一帧应该发出的整数滚动量。
    pub fn tick(&mut self, dt: f64) -> i32 {
        let mut step = self.remaining * (1.0 - (-dt / TAU).exp());
        let min = MIN_SPEED * dt;
        if step.abs() < min {
            step = min.copysign(self.remaining);
        }
        if step.abs() >= self.remaining.abs() {
            step = self.remaining; // 走完了
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
        // 收尾不能拖：一格滚轮要在 0.3 秒内走完
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
