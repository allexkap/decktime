mod db;
mod observer;
mod schedule;

use clap::Parser;
use log::info;
use std::{
    cell::RefCell,
    cmp,
    rc::Rc,
    sync::{atomic, Arc},
    thread,
    time::{Duration, SystemTime},
};

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Args {
    #[arg(short, default_value = ":memory:")]
    #[arg(value_name = "PATH", help = "Path to the database")]
    db_path: String,

    #[arg(short, default_value = "1", value_parser = parse_secs)]
    #[arg(value_name = "INTERVAL", help = "Update interval in seconds")]
    update_interval: Duration,

    #[arg(short, default_value = "60", value_parser = parse_secs)]
    #[arg(value_name = "INTERVAL", help = "Commit interval in seconds")]
    commit_interval: Duration,
}

fn parse_secs(s: &str) -> Result<Duration, String> {
    match s.parse::<u64>() {
        Ok(val) => Ok(Duration::from_secs(val)),
        Err(err) => Err(err.to_string()),
    }
}

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
    env_logger::builder().format_timestamp(None).init();
    info!("version {}", env!("CARGO_PKG_VERSION"));

    let args = Args::parse();

    let mut db = db::DeckDB::build(&args.db_path).unwrap();

    let now = SystemTime::now();
    db.commit(now).unwrap();

    let ref_db1 = Rc::new(RefCell::new(db));
    let ref_db2 = Rc::clone(&ref_db1);

    let mut sched = schedule::Scheduler::build_aligned(
        vec![
            (
                args.commit_interval,
                Box::new(move |x| ref_db1.borrow_mut().commit(x).expect("commit error")),
            ),
            (
                args.update_interval,
                Box::new(observer::get_update_func(
                    args.update_interval.as_secs(),
                    ref_db2,
                )),
            ),
        ],
        now,
    );

    let term = Arc::new(atomic::AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(sig, Arc::clone(&term)).unwrap();
    }
    while !term.load(atomic::Ordering::Relaxed) {
        let now = real_sleep(sched.get_next_timestamp().unwrap(), args.update_interval);
        sched.run_pending(now);
    }

    info!("exiting");
}
