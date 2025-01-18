use crate::db;
use itertools::Itertools;
use log::info;
use std::{cell::RefCell, fs, rc::Rc, time::SystemTime};

const PROC_PATH: &str = "/proc";
const APP_NAME: &str = "steam";

fn find_pid_by_name(appname: &str) -> Option<u32> {
    fs::read_dir(PROC_PATH)
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
            ppid = find_pid_by_name(APP_NAME);
            let Some(ppid) = ppid else {
                return;
            };
            info!("{APP_NAME} pid = {ppid}");
        }

        if let Ok(dir_entry) = fs::read_dir(format!("{PROC_PATH}/{}/task", ppid.unwrap())) {
            dir_entry
                .filter_map(Result::ok)
                .filter_map(|entry| fs::read_to_string(entry.path().join("children")).ok())
                .flat_map(|pids| {
                    pids.split_ascii_whitespace()
                        .filter_map(|pid| pid.parse::<u32>().ok())
                        .collect::<Vec<u32>>()
                })
                .filter_map(|pid| {
                    let cmdline = fs::read_to_string(format!("{PROC_PATH}/{pid}/cmdline")).ok()?;
                    let pos = cmdline.find("AppId=")?;
                    let len = cmdline[pos..].find("\x00")?;
                    cmdline[pos + 6..pos + len].parse::<u64>().ok()
                })
                .unique()
                .for_each(|app_id| ref_db.borrow_mut().update(app_id, value));
            return;
        }
        info!("{APP_NAME} pid not found");
        ppid = None;
    };
}
