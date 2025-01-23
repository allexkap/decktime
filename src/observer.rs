use crate::db;
use log::{debug, error, info, warn};
use std::{cell::RefCell, fs, rc::Rc, time::SystemTime};

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

    return move |now| {
        let mut db = ref_db.borrow_mut();

        if ppid.is_none() {
            ppid = find_pid_by_name("steam");
            if ppid.is_none() {
                return;
            };
            info!("steam pid={}", ppid.unwrap());
            db.event(now, db::STEAM_APP_ID, db::EventType::Started)
                .expect("event error");
        }

        let Ok(dir) = fs::read_dir(format!("/proc/{}/task", ppid.unwrap())) else {
            warn!("steam pid not found");
            db.event(now, db::STEAM_APP_ID, db::EventType::Stopped)
                .expect("event error");
            ppid = None;
            return;
        };

        let mut closed_apps = db.get_running_apps();

        dir.filter_map(|entry| fs::read_to_string(entry.ok()?.path().join("children")).ok())
            .flat_map(|pids| {
                pids.split_ascii_whitespace()
                    .filter_map(|pid| pid.parse().ok())
                    .collect::<Vec<u32>>()
            })
            .for_each(|pid| {
                if let Some(app_id) = get_app_id_by_pid(pid) {
                    if closed_apps.remove(&app_id) == false {
                        db.event(now, app_id, db::EventType::Started)
                            .expect("event error");
                    }
                    db.update(app_id, value);
                }
            });

        closed_apps.iter().for_each(|&app_id| {
            // idk how else to skip special values...
            if app_id > 10 {
                db.event(now, app_id, db::EventType::Stopped)
                    .expect("event error");
            }
        });
    };
}
