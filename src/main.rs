use std::cmp::min;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    callback: fn(SystemTime) -> (),
}

impl Timer {
    fn build_aligned(delay: Duration, callback: fn(SystemTime) -> (), now: SystemTime) -> Timer {
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
                self.next_timestamp = get_next_ts(self.next_timestamp, now, self.delay);
            }
        }
    }
}

struct Scheduler {
    timers: Vec<Timer>,
    next_timestamp: Option<SystemTime>,
}

impl Scheduler {
    fn build_aligned(params: Vec<(Duration, fn(SystemTime) -> ())>, now: SystemTime) -> Scheduler {
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

    fn run_pending(&mut self, now: SystemTime) {
        self.next_timestamp = self
            .timers
            .iter_mut()
            .map(|timer| {
                timer.check(now);
                timer.next_timestamp
            })
            .min();
    }
}

fn real_sleep(until: SystemTime, interval: Duration) -> SystemTime {
    loop {
        let now = SystemTime::now();
        match until.duration_since(now) {
            Ok(duration) => thread::sleep(min(duration, interval)),
            Err(_) => return now,
        }
    }
}

fn main() {
    let mut sched = Scheduler::build_aligned(
        vec![
            (Duration::from_secs(10), |x| println!("commit {x:?}")),
            (Duration::from_secs(1), |x| println!("update {x:?}")),
        ],
        SystemTime::now(),
    );
    loop {
        let now = real_sleep(sched.next_timestamp.unwrap(), Duration::from_secs(1));
        sched.run_pending(now);
    }
}
