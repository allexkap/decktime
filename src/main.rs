mod db;
mod observer;
mod schedule;

use std::{
    cell::RefCell,
    cmp,
    rc::Rc,
    thread,
    time::{Duration, SystemTime},
};

fn real_sleep(until: SystemTime, interval: Duration) -> SystemTime {
    loop {
        let now = SystemTime::now();
        match until.duration_since(now) {
            Ok(duration) => thread::sleep(cmp::min(duration, interval)),
            Err(_) => return now,
        }
    }
}

fn main() {
    env_logger::builder().format_timestamp_millis().init();

    let mut db = db::DeckDB::build(":memory:").unwrap();

    let now = SystemTime::now();
    db.commit(now).unwrap();

    let ref_db1 = Rc::new(RefCell::new(db));
    let ref_db2 = Rc::clone(&ref_db1);

    let mut sched = schedule::Scheduler::build_aligned(
        vec![
            (
                Duration::from_secs(10),
                Box::new(move |x| ref_db1.borrow_mut().commit(x).unwrap()),
            ),
            (
                Duration::from_secs(1),
                Box::new(observer::get_update_func(ref_db2)),
            ),
        ],
        now,
    );

    loop {
        let now = real_sleep(sched.get_next_timestamp().unwrap(), Duration::from_secs(1));
        sched.run_pending(now);
    }
}
