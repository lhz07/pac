use crate::{
    PAC_PATH,
    brew_api::{BottleInfo, PacInfo},
    errors::CatError,
    macos::version::ARCH,
    package::script::Pac,
    sql,
};
use sqlx::{Decode, Encode, Pool, Sqlite, SqlitePool, prelude::Type, sqlite::SqliteConnectOptions};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
    time::UNIX_EPOCH,
};

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

static SQL_OPTS: LazyLock<SqliteConnectOptions> = LazyLock::new(|| {
    SqliteConnectOptions::new()
        .filename(format!("{PAC_PATH}/PacData/pacs.sqlite"))
        .create_if_missing(true)
});

static SQL_POOL: LazyLock<Pool<Sqlite>> =
    LazyLock::new(|| Pool::connect_lazy_with(SQL_OPTS.clone()));

pub struct SqlTransaction {
    pub tx: sqlx::Transaction<'static, Sqlite>,
}

impl SqlTransaction {
    pub async fn new() -> Result<Self, CatError> {
        let tx = SQL_POOL.begin().await?;
        Ok(Self { tx })
    }
    pub async fn commit(self) -> Result<(), CatError> {
        self.tx.commit().await?;
        Ok(())
    }

    pub async fn is_installed(&mut self, name: &str) -> Result<Option<(i64, PacState)>, CatError> {
        let id_state = sqlx::query_as::<_, (i64, PacState)>(sql::SELECT_PAC_ID)
            .bind(name)
            .bind(PAC_PATH)
            .fetch_optional(&mut *self.tx)
            .await?;
        Ok(id_state)
    }

    pub async fn get_pac_name(&mut self, id: i64) -> Result<String, CatError> {
        let name: String = sqlx::query_scalar(sql::SELECT_PAC_NAME)
            .bind(id)
            .fetch_one(&mut *self.tx)
            .await?;
        Ok(name)
    }

    pub async fn get_pac_names(&mut self) -> Result<Vec<String>, CatError> {
        let names: Vec<String> = sqlx::query_scalar(sql::SELECT_PAC_NAMES)
            .fetch_all(&mut *self.tx)
            .await?;
        Ok(names)
    }

    pub async fn get_pacs(&mut self, explict: bool) -> Result<Vec<String>, CatError> {
        let names: Vec<String> = sqlx::query_scalar(sql::SELECT_PACS)
            .bind(explict as u8)
            .fetch_all(&mut *self.tx)
            .await?;
        Ok(names)
    }

    pub async fn get_installed_files(&mut self, id: i64) -> Result<Vec<PathBuf>, CatError> {
        let file_list: Vec<String> = sqlx::query_scalar(sql::SELECT_INSTALLED_FILE)
            .bind(id)
            .fetch_all(&mut *self.tx)
            .await?;
        let path_list = file_list.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        Ok(path_list)
    }

    pub async fn get_reverse_deps(&mut self, name: &str) -> Result<Vec<String>, CatError> {
        let rev_deps: Vec<i64> = sqlx::query_scalar(sql::SELECT_REVERSE_DEP)
            .bind(name)
            .fetch_all(&mut *self.tx)
            .await?;
        let mut deps_name = Vec::new();
        for id in rev_deps {
            let name = self.get_pac_name(id).await?;
            deps_name.push(name);
        }
        Ok(deps_name)
    }

    pub async fn is_path_exist<P>(&mut self, path: P) -> Result<bool, CatError>
    where
        P: AsRef<Path>,
    {
        let (exists,): (i64,) = sqlx::query_as(sql::SELECT_EXIST_FILE)
            .bind(path.as_ref().to_string_lossy())
            .fetch_one(&mut *self.tx)
            .await?;
        Ok(exists == 1)
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

    pub async fn get_orphan_pacs(&mut self) -> Result<Vec<(i64, String, PacState)>, CatError> {
        let rows = sqlx::query_as::<_, (i64, String, PacState)>(sql::SELECT_ORPHAN_PAC)
            .fetch_all(&mut *self.tx)
            .await?;
        Ok(rows)
    }
}
