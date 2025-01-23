use log::{debug, error, info, trace};
use rusqlite::{Connection, Error, Result};
use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

pub type AppId = u32;
pub const THIS_APP_ID: AppId = 0;

#[derive(Debug, Clone, Copy)]
pub enum EventType {
    Running = 0,
    Started,
    Stopped,
    Suspended,
    Resumed,
}

struct AppCache {
    apps: HashMap<AppId, u64>,
    timestamp_h: u64,
}

pub struct DeckDB {
    conn: Connection,
    cache: AppCache,
    running_apps: HashSet<AppId>,
}

impl DeckDB {
    pub fn build(path: &str, timestamp: SystemTime) -> Result<DeckDB> {
        let mut conn = Connection::open(path)?;

        let tx = conn.transaction()?;
        tx.execute(
            "create table if not exists objects ( \
                object_id integer not null, \
                app_id integer unique not null, \
                alias text, \
                primary key (object_id)
            );",
            (),
        )?;
        tx.execute(
            "create table if not exists timeline ( \
                timestamp integer not null, \
                object_id integer not null, \
                value integer not null, \
                primary key (timestamp, object_id), \
                foreign key (object_id) references objects (object_id)
            )",
            (),
        )?;

        tx.execute(
            "create table if not exists events ( \
                timestamp integer not null, \
                object_id integer not null, \
                event_type integer not null, \
                foreign key (object_id) references objects (object_id)
            )",
            (),
        )?;
        assert_eq!(EventType::Running as u32, 0);
        tx.execute(
            "update events set event_type = ?1 where event_type = ?2",
            (EventType::Stopped as u32, EventType::Running as u32),
        )?;

        tx.commit()?;

        let mut db = DeckDB {
            conn,
            cache: AppCache {
                apps: HashMap::new(),
                timestamp_h: 0,
            },
            running_apps: HashSet::new(),
        };
        db.load_cache(to_unix_ts(timestamp) / 60 / 60);
        db.event(timestamp, None, EventType::Started)?;

        debug!("database {path:?} opened successfully");

        Ok(db)
    }

    fn get_object_id(conn: &Connection, app_id: AppId) -> Result<u32> {
        conn.query_row(
            "select object_id from objects where app_id = ?1",
            (app_id,),
            |row| row.get(0),
        )
        .or_else(|err| match err {
            Error::QueryReturnedNoRows => conn.query_row(
                "insert into objects (app_id) values (?1) returning object_id",
                (app_id,),
                |row| row.get(0),
            ),
            err => Err(err),
        })
    }

    fn load_cache(&mut self, timestamp_h: u64) {
        debug!("loading cache with timestamp={timestamp_h}");

        let mut stmt = self
            .conn
            .prepare_cached(
                "select app_id, value from timeline \
                join objects on timeline.object_id = objects.object_id \
                where timestamp = ?1",
            )
            .expect("sql call failed");

        let apps = stmt
            .query_map((timestamp_h,), |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("sql binding failed")
            .filter_map(Result::ok)
            .collect();

        self.cache = AppCache { apps, timestamp_h };
    }

    fn dump_cache(&mut self) -> Result<()> {
        debug!("dumping cache with timestamp={}", self.cache.timestamp_h);

        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx
                .prepare_cached("insert or replace into timeline values (?1, ?2, ?3)")
                .expect("sql call failed");
            for (&app_id, &value) in self.cache.apps.iter() {
                let object_id = Self::get_object_id(&(*tx), app_id)?;
                stmt.execute((self.cache.timestamp_h, object_id, value))?;
            }
        }

        tx.commit()
    }

    pub fn update(&mut self, app_id: AppId, value: u64) {
        trace!("update with app_id={app_id} value={value}");

        match self.cache.apps.get_mut(&app_id) {
            Some(entry) => *entry += value,
            None => {
                self.cache.apps.insert(app_id, value);
            }
        }
    }

    pub fn commit(&mut self, timestamp: SystemTime) -> Result<()> {
        let timestamp_h = to_unix_ts(timestamp) / 60 / 60;
        debug!("commit with timestamp={timestamp_h}");

        self.event(timestamp, None, EventType::Running)?;

        self.dump_cache()?;
        if timestamp_h != self.cache.timestamp_h {
            info!("reload cache with timestamp={timestamp_h}");
            self.load_cache(timestamp_h);
        }

        Ok(())
    }

    pub fn event(
        &mut self,
        timestamp: SystemTime,
        app_id: Option<AppId>,
        event_type: EventType,
    ) -> Result<()> {
        let timestamp_s = to_unix_ts(timestamp);

        let app_id = match app_id {
            Some(app_id) => {
                info!("new event: app_id={app_id}; event_type={event_type:?}");
                app_id
            }
            None => THIS_APP_ID,
        };

        const SQL_INSERT: &'static str =
            "insert into events (timestamp, object_id, event_type) values (?1, ?2, ?3)";
        match event_type {
            EventType::Started | EventType::Stopped => {
                let object_id = Self::get_object_id(&self.conn, app_id)?;
                self.conn
                    .execute(SQL_INSERT, (timestamp_s, object_id, event_type as u32))?;
            }

            EventType::Suspended | EventType::Resumed | EventType::Running => {
                let tx = self.conn.transaction()?;
                if let EventType::Running = event_type {
                    assert_eq!(EventType::Running as u32, 0);
                    tx.execute(
                        "delete from events where event_type = ?1",
                        (EventType::Running as u32,),
                    )?;
                }
                {
                    let mut stmt = tx.prepare_cached(SQL_INSERT)?;
                    for &app in self.running_apps.iter() {
                        stmt.execute((
                            timestamp_s,
                            Self::get_object_id(&(*tx), app)?,
                            event_type as u32,
                        ))?;
                    }
                }
                tx.commit()?;
            }
        }

        let ok = match event_type {
            EventType::Started => self.running_apps.insert(app_id),
            EventType::Stopped => self.running_apps.remove(&app_id),
            _ => true,
        };

        if !ok {
            error!("duplicated event: app_id={app_id}; event_type={event_type:?}");
        }

        Ok(())
    }

    pub fn flush(&mut self, timestamp: SystemTime) -> Result<()> {
        self.dump_cache()?;
        for app_id in self.running_apps.clone() {
            self.event(timestamp, Some(app_id), EventType::Stopped)?;
        }
        Ok(())
    }
}

impl Drop for DeckDB {
    fn drop(&mut self) {
        if !self.running_apps.is_empty() {
            error!("running_apps is not empty");
        }
    }
}

fn to_unix_ts(timestamp: SystemTime) -> u64 {
    timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs()
}
