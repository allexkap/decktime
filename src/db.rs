use log::info;
use rusqlite::{Connection, Error, Result};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct DeckDB {
    conn: Connection,
    cache: Option<HashMap<u64, u64>>,
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

    pub fn update(&mut self, app_id: u64, value: u64) {
        info!("update with app_id={app_id} value={value}");

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
        info!("commit with timestamp={timestamp}");

        let tx = self.conn.transaction()?;
        if let Some(cache) = self.cache.as_ref() {
            for (&app_id, &value) in cache.iter() {
                let object_id: u64 = match tx.query_row(
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
                    [self.cache_timestamp, object_id, value],
                )?;
            }
        }

        if self.cache.is_none() || timestamp != self.cache_timestamp {
            info!("reload cache");
            let mut stmt = tx.prepare_cached(
                "select app_id, value from timeline
                join objects on timeline.object_id = objects.object_id
                where timestamp = ?1",
            )?;

            let cache: HashMap<u64, u64> = stmt
                .query_map([timestamp], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(Result::ok)
                .collect();

            self.cache = Some(cache);
            self.cache_timestamp = timestamp;
        }

        tx.commit()
    }
}
