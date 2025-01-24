use crate::db;
use log::info;
use std::{
    cell::RefCell,
    collections::HashSet,
    fs,
    rc::Rc,
    time::{Duration, SystemTime},
};

fn find_pid_by_name(appname: &str) -> Option<u32> {
    fs::read_dir("/proc")
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            fs::read_to_string(entry.path().join("comm"))
                .is_ok_and(|s| s[..s.len() - 1] == *appname)
        })
        .filter_map(|entry| entry.path().file_name()?.to_str()?.parse().ok())
        .next()
}

fn get_app_id_by_pid(pid: u32) -> Option<u32> {
    let cmdline = fs::read_to_string(format!("/proc/{pid}/cmdline")).ok()?;
    let pos = cmdline.find("AppId=")? + 6;
    let len = cmdline[pos..].find("\x00")?;
    cmdline[pos..pos + len].parse::<u32>().ok()
}

pub fn get_update_func(value: u64, ref_db: Rc<RefCell<db::DeckDB>>) -> impl FnMut(SystemTime) {
    let mut ppid = None;
    let mut apps = HashSet::<u32>::new();

    return move |now| {
        if ppid.is_none() {
            ppid = find_pid_by_name("steam");
            if ppid.is_none() {
                return;
            };
            info!("steam pid={}", ppid.unwrap());
        }

        let mut db = ref_db.borrow_mut();

        let Ok(dir) = fs::read_dir(format!("/proc/{}/task", ppid.unwrap())) else {
            info!("steam pid not found");
            apps.drain().for_each(|app_id| {
                db.event(now, Some(app_id), db::EventType::Stopped)
                    .expect("event error")
            });
            ppid = None;
            return;
        };

        let mut closed_apps = apps.clone();

        dir.filter_map(|entry| fs::read_to_string(entry.ok()?.path().join("children")).ok())
            .flat_map(|pids| {
                pids.split_ascii_whitespace()
                    .filter_map(|pid| pid.parse().ok())
                    .collect::<Vec<_>>()
            })
            .filter_map(get_app_id_by_pid)
            .for_each(|app_id| {
                if apps.insert(app_id) == true {
                    db.event(now, Some(app_id), db::EventType::Started)
                        .expect("event error");
                } else if closed_apps.remove(&app_id) == true {
                    db.update(app_id, value);
                } else {
                    info!("duplicated app_id={app_id}");
                }
            });

        closed_apps.into_iter().for_each(|app_id| {
            apps.remove(&app_id);
            db.event(now, Some(app_id), db::EventType::Stopped)
                .expect("event error");
        });
    };
}

pub fn get_suspend_check_func(
    max_duration: Duration,
    ref_db: Rc<RefCell<db::DeckDB>>,
) -> impl FnMut(SystemTime) {
    let mut prev_ts = None;
    move |now| {
        if let Some(prev_ts) = prev_ts {
            if now.duration_since(prev_ts).expect("time ne tuda") > max_duration {
                let mut db = ref_db.borrow_mut();
                db.event(prev_ts, None, db::EventType::Suspended)
                    .expect("event error");
                db.event(now, None, db::EventType::Resumed)
                    .expect("event error");
            }
        }
        prev_ts = Some(now);
    }
}

pub fn get_commit_func(ref_db: Rc<RefCell<db::DeckDB>>) -> impl FnMut(SystemTime) {
    move |x| ref_db.borrow_mut().commit(x).expect("commit error")
}
