use crate::{
    PAC_PATH,
    brew_api::{BottleInfo, PacInfo},
    database::{SQL_OPTS, SQL_POOL, basic::AsConnection},
    errors::CatError,
    macos::version::ARCH,
    package::script::Pac,
    sql,
};
use sqlx::{
    Decode, Encode, Sqlite, SqlitePool,
    pool::PoolConnection,
    prelude::{FromRow, Type},
};
use std::{
    fs, future,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Debug, FromRow)]
pub struct PacData {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub build_epoch: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum PacState {
    Installed,
    Broken,
}

impl Type<Sqlite> for PacState {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <i64 as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for PacState {
    fn decode(
        value: <Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <i64 as Decode<Sqlite>>::decode(value)?;
        match s {
            0 => Ok(PacState::Installed),
            1 => Ok(PacState::Broken),
            _ => Err("Invalid value for PacState".into()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PacSource {
    Brew,
    Pac,
    Local,
    ThirdParty(String),
}

impl Type<Sqlite> for PacSource {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

const SOURCE_PREFIX: &str = "third_party:";

impl<'r> Decode<'r, Sqlite> for PacSource {
    fn decode(
        value: <Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <String as Decode<Sqlite>>::decode(value)?;
        match s.as_str() {
            "brew" => Ok(PacSource::Brew),
            "pac" => Ok(PacSource::Pac),
            "local" => Ok(PacSource::Local),
            s => {
                if s.starts_with(SOURCE_PREFIX) {
                    let s = s.split_at(SOURCE_PREFIX.len()).1.to_string();
                    Ok(PacSource::ThirdParty(s))
                } else {
                    Err("Invalid value for PacSource".into())
                }
            }
        }
    }
}

impl<'r> Encode<'r, Sqlite> for PacSource {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'r>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let str = match &self {
            PacSource::Brew => "brew".to_string(),
            PacSource::Local => "local".to_string(),
            PacSource::Pac => "pac".to_string(),
            PacSource::ThirdParty(s) => format!("{}{}", SOURCE_PREFIX, s),
        };
        let value = <String as Encode<Sqlite>>::encode(str, buf)?;
        Ok(value)
    }
}

pub async fn init_db() -> Result<(), CatError> {
    let path = Path::new(PAC_PATH).join("PacData");
    if fs::metadata(path.join("pacs.sqlite")).is_err() {
        fs::create_dir_all(path)?;
        println!("Database file not found, creating a new one...");
        let pool = SqlitePool::connect_with(SQL_OPTS.clone()).await?;
        sqlx::query(sql::INIT_DB).execute(&pool).await?;
    }
    Ok(())
}

pub struct SqlTransMark {
    _p: (),
}

impl SqlTransMark {
    pub async fn get_connection(&self) -> Result<PoolConnection<Sqlite>, CatError> {
        let conn = SQL_POOL.acquire().await?;
        Ok(conn)
    }
    pub async fn into_transaction(self) -> Result<SqlTransaction, CatError> {
        let tx = SQL_POOL.begin().await?;
        Ok(SqlTransaction { tx })
    }
}

/// a sql transaction
///
/// **NOTE:** if you need read-only operation, use `PoolConnection<Sqlite>` instead
///
/// It is strongly recommended to use read-only operation when possible
///
/// **WARNING:** transaction may cause deadlock, if you modify the db by a transaction, than
/// the transaction is locked until you commit/drop it, which means you can still read the db
/// through another transaction, but any write operaion(through another transaction) will get deadlock.
pub struct SqlTransaction {
    tx: sqlx::Transaction<'static, Sqlite>,
}

impl SqlTransaction {
    /// # Safety
    /// You should ensure that there is no write transaction alive
    pub unsafe fn new_mark() -> SqlTransMark {
        SqlTransMark { _p: () }
    }
    pub fn rollback(self) -> SqlTransMark {
        drop(self.tx);
        SqlTransMark { _p: () }
    }
    pub async fn commit(self) -> Result<SqlTransMark, CatError> {
        self.tx.commit().await?;
        Ok(SqlTransMark { _p: () })
    }
    pub async fn install_a_pac(
        &mut self,
        pac: &PacInfo,
        pac_source: PacSource,
        version: &str,
        bottle: &BottleInfo,
        sha256: &str,
        explict: bool,
        installed_files: &[PathBuf],
    ) -> Result<(), CatError> {
        let time = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix epoch is always earlier than now")
            .as_secs() as i64;
        sqlx::query(sql::INSERT_PAC)
            .bind(&pac.name)
            .bind(version)
            .bind(bottle.rebuild)
            .bind(ARCH)
            .bind("stable")
            .bind(PAC_PATH)
            .bind(pac_source)
            .bind(explict as u8)
            .bind(time)
            .bind(sha256)
            .execute(&mut *self.tx)
            .await?;
        let pac_id = sqlx::query_scalar::<_, i64>(sql::SELECT_PAC_ID)
            .bind(&pac.name)
            .bind(PAC_PATH)
            .fetch_one(&mut *self.tx)
            .await?;
        for dep in &pac.dependencies {
            sqlx::query(sql::INSERT_DEP)
                .bind(pac_id)
                .bind(dep)
                .execute(&mut *self.tx)
                .await?;
        }
        for conflict in &pac.conflicts_with {
            sqlx::query(sql::INSERT_CONFLICT)
                .bind(pac_id)
                .bind(conflict)
                .execute(&mut *self.tx)
                .await?;
        }
        for file_path in installed_files {
            sqlx::query(sql::INSERT_INSTALLED_FILE)
                .bind(pac_id)
                .bind(file_path.to_string_lossy())
                .execute(&mut *self.tx)
                .await?;
        }
        Ok(())
    }

    pub async fn install_a_pac_pac(
        &mut self,
        pac: &Pac,
        pac_source: PacSource,
        explict: bool,
        installed_files: &[PathBuf],
    ) -> Result<(), CatError> {
        let time = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix epoch is always earlier than now")
            .as_secs() as i64;
        sqlx::query(sql::INSERT_PAC_PAC)
            .bind(&pac.basic.name)
            .bind(&pac.basic.version)
            .bind(ARCH)
            .bind("stable")
            .bind(PAC_PATH)
            .bind(pac_source)
            .bind(explict as u8)
            .bind(time)
            .execute(&mut *self.tx)
            .await?;
        let pac_id = sqlx::query_scalar::<_, i64>(sql::SELECT_PAC_ID)
            .bind(&pac.basic.name)
            .bind(PAC_PATH)
            .fetch_one(&mut *self.tx)
            .await?;
        for dep in pac
            .basic
            .pac_dependencies
            .iter()
            .chain(pac.basic.brew_dependencies.iter())
        {
            sqlx::query(sql::INSERT_DEP)
                .bind(pac_id)
                .bind(dep)
                .execute(&mut *self.tx)
                .await?;
        }
        for conflict in pac.conflicts.keys() {
            sqlx::query(sql::INSERT_CONFLICT)
                .bind(pac_id)
                .bind(conflict)
                .execute(&mut *self.tx)
                .await?;
        }
        for file_path in installed_files {
            sqlx::query(sql::INSERT_INSTALLED_FILE)
                .bind(pac_id)
                .bind(file_path.to_string_lossy())
                .execute(&mut *self.tx)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_a_pac(&mut self, id: i64) -> Result<(), CatError> {
        sqlx::query(sql::DELETE_PAC)
            .bind(id)
            .execute(&mut *self.tx)
            .await?;
        Ok(())
    }

    pub async fn update_pac_state(&mut self, id: i64, state: PacState) -> Result<(), CatError> {
        sqlx::query(sql::UPDATE_PAC_STATE)
            .bind(state as i64)
            .bind(id)
            .execute(&mut *self.tx)
            .await?;

        Ok(())
    }
}

impl Deref for SqlTransaction {
    type Target = sqlx::Transaction<'static, Sqlite>;

    fn deref(&self) -> &Self::Target {
        &self.tx
    }
}

impl DerefMut for SqlTransaction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.tx
    }
}

impl AsConnection for SqlTransaction {
    fn to_conn_type(
        &mut self,
    ) -> impl Future<Output = Result<super::basic::ConnType<'_>, CatError>> {
        future::ready(Ok(super::basic::ConnType::Transaction(&mut self.tx)))
    }
}

#[tokio::test]
async fn test_deadlock() {
    use crate::database::basic::{SqlPool, SqlRead};
    let mut a = SqlPool;
    let path_list = a.get_installed_files(1).await.unwrap();
    println!("{:?}", path_list);
    let tx = SQL_POOL.begin().await.unwrap();
    let mut b = SqlTransaction { tx };
    println!("{:?}", b.delete_a_pac(6).await);
    b.commit().await.unwrap();
}
