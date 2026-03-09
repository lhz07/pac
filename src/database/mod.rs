use std::sync::LazyLock;

use sqlx::{Pool, Sqlite, sqlite::SqliteConnectOptions};

use crate::PAC_PATH;

pub mod basic;
pub mod local;
pub mod sync;

static SQL_POOL: LazyLock<Pool<Sqlite>> =
    LazyLock::new(|| Pool::connect_lazy_with(SQL_OPTS.clone()));

static SQL_OPTS: LazyLock<SqliteConnectOptions> = LazyLock::new(|| {
    SqliteConnectOptions::new()
        .filename(format!("{PAC_PATH}/PacData/pacs.sqlite"))
        .create_if_missing(true)
});
