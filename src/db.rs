use log::{debug, error, info};
use rusqlite::{Connection, Error, Result};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub type AppId = u32;

pub struct DeckDB {
    conn: Connection,
    cache: Option<HashMap<AppId, u64>>,
    cache_timestamp: u64,
}

impl DeckDB {
    pub fn build(path: &str) -> Result<DeckDB> {
        let mut conn = Connection::open(path)?;

        let tx = conn.transaction()?;
        tx.execute(
            "create table if not exists objects (
                object_id integer not null,
                app_id integer unique not null,
                alias text,
                primary key (object_id)
            );",
            (),
        )?;
        tx.execute(
            "create table if not exists timeline (
                timestamp integer not null,
                object_id integer not null,
                value integer not null,
                primary key (timestamp, object_id),
                foreign key (object_id) references objects (object_id)
            )",
            (),
        )?;
        tx.commit()?;

        Ok(DeckDB {
            conn,
            cache: None,
            cache_timestamp: 0,
        })
    }

    fn load_cache(&mut self, timestamp: u64) {
        let mut stmt = self
            .conn
            .prepare_cached(
                "select app_id, value from timeline
                join objects on timeline.object_id = objects.object_id
                where timestamp = ?1",
            )
            .expect("sql syntax error");

        let cache = stmt
            .query_map((timestamp,), |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("database error that should not happen")
            .filter_map(Result::ok)
            .collect();

        self.cache = Some(cache);
        self.cache_timestamp = timestamp;
    }

    fn save_cache(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        if let Some(cache) = self.cache.as_ref() {
            for (&app_id, &value) in cache.iter() {
                let object_id: u32 = match tx.query_row(
                    "select object_id from objects where app_id = ?1",
                    (app_id,),
                    |row| row.get(0),
                ) {
                    Ok(entry) => entry,
                    Err(Error::QueryReturnedNoRows) => tx.query_row(
                        "insert into objects (app_id) values (?1) returning object_id",
                        (app_id,),
                        |row| row.get(0),
                    )?,
                    Err(err) => {
                        panic!("{err}");
                    }
                };
                tx.execute(
                    "insert or replace into timeline values (?1, ?2, ?3)",
                    (self.cache_timestamp, object_id, value),
                )?;
            }
        }
        tx.commit()
    }

    pub fn update(&mut self, app_id: AppId, value: u64) {
        debug!("update with app_id={app_id} value={value}");

        let cache = self.cache.as_mut().expect("cache not initialized");
        match cache.get_mut(&app_id) {
            Some(entry) => *entry += value,
            None => {
                cache.insert(app_id, value);
            }
        }
    }

    pub fn commit(&mut self, timestamp: SystemTime) -> Result<()> {
        let timestamp = timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs() / 60 / 60;
        debug!("commit with timestamp={timestamp}");

        self.save_cache()?;

        if self.cache.is_none() || timestamp != self.cache_timestamp {
            info!("reload cache");
            self.load_cache(timestamp);
        }

        Ok(())
    }
}

impl Drop for DeckDB {
    fn drop(&mut self) {
        match self.save_cache() {
            Ok(_) => info!("database dropped successfully"),
            Err(err) => error!("database drop error: {err}"),
        }
    }
}
