use log::info;
use rusqlite::{Connection, Error, Result};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct AppDB {
    conn: Connection,
    cache: Option<HashMap<String, u64>>,
    cache_timestamp: u64,
}

impl AppDB {
    pub fn build(path: &str) -> Result<AppDB> {
        let conn = Connection::open(path)?;

        conn.execute(
            "create table if not exists objects (
                object_id integer not null,
                name text unique not null,
                alias text,
                primary key (object_id)
            );",
            (),
        )?;
        conn.execute(
            "create table if not exists content (
                timestamp integer not null,
                object_id integer not null,
                value integer not null,
                primary key (timestamp, object_id),
                foreign key (object_id) references objects (object_id)
            )",
            (),
        )?;

        Ok(AppDB {
            conn,
            cache: None,
            cache_timestamp: 0,
        })
    }

    pub fn update(&mut self, name: &str, value: u64) {
        info!("update with name={name} value={value}");

        let cache = self.cache.as_mut().expect("cache not initialized");
        match cache.get_mut(name) {
            Some(entry) => *entry += value,
            None => {
                cache.insert(name.into(), value);
            }
        }
    }

    pub fn commit(&mut self, timestamp: SystemTime) -> Result<()> {
        let timestamp = timestamp.duration_since(UNIX_EPOCH).unwrap().as_secs() / 60 / 60;
        info!("commit with timestamp={timestamp}");

        let tx = self.conn.transaction()?;
        if let Some(cache) = self.cache.as_ref() {
            for (name, &value) in cache.iter() {
                let object_id: u64 = match tx.query_row(
                    "select object_id from objects where name = ?1",
                    (name,),
                    |row| row.get(0),
                ) {
                    Ok(entry) => entry,
                    Err(Error::QueryReturnedNoRows) => tx.query_row(
                        "insert into objects (name) values (?1) returning object_id",
                        (name,),
                        |row| row.get(0),
                    )?,
                    Err(err) => {
                        panic!("{err}");
                    }
                };
                tx.execute(
                    "insert or replace into content values (?1, ?2, ?3)",
                    [self.cache_timestamp, object_id, value],
                )?;
            }
        }

        if self.cache.is_none() || timestamp != self.cache_timestamp {
            info!("reload cache");
            let mut stmt = tx.prepare_cached(
                "select name, value from content
                join objects on content.object_id = objects.object_id
                where timestamp = ?1",
            )?;

            let cache: HashMap<String, u64> = stmt
                .query_map([timestamp], |row| Ok((row.get(0)?, row.get(1)?)))?
                .filter_map(Result::ok)
                .collect();

            self.cache = Some(cache);
            self.cache_timestamp = timestamp;
        }

        tx.commit()
    }
}
