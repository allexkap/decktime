use crate::db;
use std::{cell::RefCell, fs, rc::Rc, time::SystemTime};

const APP_NAME: &str = "fish";

fn find_pid_by_name(name: &str) -> Option<u32> {
    for entry in fs::read_dir("/proc").ok()? {
        if let Ok(entry) = entry {
            let path = entry.path();
            if let Ok(pid) = path.file_name().unwrap().to_string_lossy().parse() {
                if let Ok(filename) = fs::read_to_string(path.join("comm")) {
                    if filename[0..filename.len() - 1] == *name {
                        return Some(pid);
                    }
                }
            }
        }
    }
    None
}

pub fn get_update_func(ref_db: Rc<RefCell<db::AppDB>>) -> impl FnMut(SystemTime) {
    // ne hochu allokacij...
    let mut path_comm = String::new();
    let mut path_children = String::new();
    let mut ppid = None;

    return move |_| {
        if ppid.is_none() {
            ppid = find_pid_by_name(APP_NAME);
            let Some(ppid) = ppid else {
                return;
            };
            println!("ppid = {ppid}");
            path_comm = format!("/proc/{}/comm", ppid);
            path_children = format!("/proc/{ppid}/task/{ppid}/children",);
        }
        if let Ok(filename) = fs::read_to_string(&path_comm) {
            if filename[0..filename.len() - 1] == *APP_NAME {
                if let Ok(pids) = fs::read_to_string(&path_children) {
                    for pid in pids.split(' ').map(|pid| pid.parse::<u32>()).flatten() {
                        if let Ok(mut filename) = fs::read_to_string(format!("/proc/{}/comm", pid))
                        {
                            filename.pop();
                            ref_db.borrow_mut().update(&filename, 1);
                        }
                    }
                    return;
                }
            }
        }
        ppid = None;
    };
}
