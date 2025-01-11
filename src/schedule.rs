use log::warn;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type Callback = Box<dyn Fn(SystemTime)>;

fn get_next_ts(start: SystemTime, now: SystemTime, step: Duration) -> SystemTime {
    start
        + step.mul_f64(
            now.duration_since(start)
                .unwrap()
                .div_duration_f64(step)
                .ceil(),
        )
}

struct Timer {
    delay: Duration,
    next_timestamp: SystemTime,
    callback: Callback,
}

impl Timer {
    fn build_aligned(delay: Duration, callback: Callback, now: SystemTime) -> Timer {
        Timer {
            delay,
            next_timestamp: get_next_ts(UNIX_EPOCH, now, delay),
            callback,
        }
    }

    fn check(&mut self, now: SystemTime) {
        if self.next_timestamp <= now {
            (self.callback)(now);
            self.next_timestamp += self.delay;
            if self.next_timestamp <= now {
                warn!(
                    "missed {}s",
                    now.duration_since(self.next_timestamp).unwrap().as_secs()
                );
                self.next_timestamp = get_next_ts(self.next_timestamp, now, self.delay);
            }
        }
    }
}

pub struct Scheduler {
    timers: Vec<Timer>,
    next_timestamp: Option<SystemTime>,
}

impl Scheduler {
    pub fn build_aligned(params: Vec<(Duration, Callback)>, now: SystemTime) -> Scheduler {
        let timers: Vec<Timer> = params
            .into_iter()
            .map(|(delay, callback)| Timer::build_aligned(delay, callback, now))
            .collect();
        let next_timestamp = timers.iter().map(|timer| timer.next_timestamp).min();
        Scheduler {
            timers,
            next_timestamp,
        }
    }

    pub fn run_pending(&mut self, now: SystemTime) {
        self.next_timestamp = self
            .timers
            .iter_mut()
            .map(|timer| {
                timer.check(now);
                timer.next_timestamp
            })
            .min();
    }

    pub fn get_next_timestamp(&self) -> Option<SystemTime> {
        return self.next_timestamp;
    }
}
