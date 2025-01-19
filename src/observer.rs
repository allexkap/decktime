use crate::db;
use itertools::Itertools;
use log::{info, warn};
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

pub fn get_update_func(value: u64, ref_db: Rc<RefCell<db::DeckDB>>) -> impl FnMut(SystemTime) {
    let mut ppid = None;
    return move |_| {
        if ppid.is_none() {
            ppid = find_pid_by_name("steam");
            let Some(ppid) = ppid else {
                return;
            };
            info!("steam pid = {ppid}");
        }

        let Ok(dir_entry) = fs::read_dir(format!("/proc/{}/task", ppid.unwrap())) else {
            info!("steam pid not found");
            ppid = None;
            return;
        };

        dir_entry
            .filter_map(Result::ok)
            .filter_map(|entry| fs::read_to_string(entry.path().join("children")).ok())
            .flat_map(|pids| {
                pids.split_ascii_whitespace()
                    .filter_map(|pid| pid.parse::<u32>().ok())
                    .collect::<Vec<u32>>()
            })
            .filter_map(|pid| {
                let cmdline = fs::read_to_string(format!("/proc/{pid}/cmdline")).ok()?;
                let pos = cmdline.find("AppId=")? + 6;
                let len = cmdline[pos..].find("\x00")?;
                match cmdline[pos..pos + len].parse::<u32>() {
                    Ok(val) => Some(val),
                    Err(err) => {
                        warn!("parse app_id error: {err}; cmdline={cmdline:?}");
                        None
                    }
                }
            })
            .unique()
            .for_each(|app_id| ref_db.borrow_mut().update(app_id, value));
    };
}
