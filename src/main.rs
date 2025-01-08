use std::cmp::min;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn get_next_ts(start: SystemTime, now: SystemTime, step: Duration) -> SystemTime {
    let delay_secs = step.as_secs();
    let cycles = now.duration_since(start).unwrap().as_secs() / delay_secs + 1;
    start + Duration::from_secs(delay_secs * cycles)
}

struct Timer {
    delay: Duration,
    next_timestamp: SystemTime,
    callback: fn(SystemTime) -> (),
}

impl Timer {
    fn build(delay: Duration, start: SystemTime, callback: fn(SystemTime) -> ()) -> Timer {
        Timer {
            delay,
            next_timestamp: start,
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
    fn build(params: Vec<(Duration, Option<SystemTime>, fn(SystemTime) -> ())>) -> Scheduler {
        let now = SystemTime::now(); // del
        let timers: Vec<Timer> = params
            .into_iter()
            .map(|(delay, start, callback)| {
                Timer::build(
                    delay,
                    start.unwrap_or(get_next_ts(UNIX_EPOCH, now, delay)),
                    callback,
                )
            })
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

fn main() {
    let mut sched = Scheduler::build(vec![
        (Duration::from_secs(10), None, |x| println!("commit {x:?}")),
        (Duration::from_secs(1), None, |x| println!("update {x:?}")),
    ]);
    loop {
        let now = SystemTime::now();
        sched.run_pending(now);
        thread::sleep(min(
            sched.next_timestamp.unwrap().duration_since(now).unwrap(),
            Duration::from_millis(1300),
        ));
    }
}
