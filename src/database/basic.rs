use crate::{
    PAC_PATH,
    database::{
        SQL_POOL,
        local::{PacData, PacSource, PacState, SqlTransaction},
    },
    errors::CatError,
    sql,
};
use sqlx::{Sqlite, SqliteConnection, pool::PoolConnection};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub struct SqlPool;

pub enum ConnType<'a> {
    Pool(PoolConnection<Sqlite>),
    Transaction(&'a mut SqliteConnection),
}

impl<'a> ConnType<'a> {
    pub fn as_connection(&mut self) -> &mut SqliteConnection {
        match self {
            ConnType::Pool(conn) => &mut *conn,
            ConnType::Transaction(conn) => conn,
        }
    }
}

pub trait AsConnection {
    fn to_conn_type(&mut self) -> impl Future<Output = Result<ConnType<'_>, CatError>>;
}

impl AsConnection for SqlPool {
    async fn to_conn_type(&mut self) -> Result<ConnType<'_>, CatError> {
        let conn = SQL_POOL.acquire().await?;
        Ok(ConnType::Pool(conn))
    }
}

impl SqlRead for SqlTransaction {}
impl SqlRead for SqlPool {}

pub trait SqlRead: AsConnection {
    fn is_installed(
        &mut self,
        name: &str,
    ) -> impl Future<Output = Result<Option<(i64, PacState)>, CatError>> {
        async move {
            let id_state = sqlx::query_as::<_, (i64, PacState)>(sql::SELECT_PAC_ID)
                .bind(name)
                .bind(PAC_PATH)
                .fetch_optional(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(id_state)
        }
    }

    fn get_pac_name(&mut self, id: i64) -> impl Future<Output = Result<String, CatError>> {
        async move {
            let name: String = sqlx::query_scalar(sql::SELECT_PAC_NAME)
                .bind(id)
                .fetch_one(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(name)
        }
    }

    fn get_pac_names(&mut self) -> impl Future<Output = Result<Vec<String>, CatError>> {
        async {
            let names: Vec<String> = sqlx::query_scalar(sql::SELECT_PAC_NAMES)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(names)
        }
    }

    fn get_pacs(&mut self, explict: bool) -> impl Future<Output = Result<Vec<String>, CatError>> {
        async move {
            let names: Vec<String> = sqlx::query_scalar(sql::SELECT_PACS)
                .bind(explict as u8)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(names)
        }
    }

    fn get_pacs_by_source(
        &mut self,
        source: PacSource,
    ) -> impl Future<Output = Result<Vec<PacData>, CatError>> {
        async move {
            let names: Vec<PacData> = sqlx::query_as(sql::SELECT_PACS_BY_SOURCE)
                .bind(source)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(names)
        }
    }

    fn get_installed_files(
        &mut self,
        id: i64,
    ) -> impl Future<Output = Result<Vec<PathBuf>, CatError>> {
        async move {
            let file_list: Vec<String> = sqlx::query_scalar(sql::SELECT_INSTALLED_FILE)
                .bind(id)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            let path_list = file_list.into_iter().map(PathBuf::from).collect::<Vec<_>>();
            Ok(path_list)
        }
    }

    fn get_all_installed_files(
        &mut self,
    ) -> impl Future<Output = Result<HashSet<PathBuf>, CatError>> {
        async {
            let file_list: Vec<String> = sqlx::query_scalar(sql::SELECT_ALL_INSTALLED_FILE)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            let path_list = file_list
                .into_iter()
                .map(PathBuf::from)
                .collect::<HashSet<_>>();
            Ok(path_list)
        }
    }

    fn get_reverse_deps(
        &mut self,
        name: &str,
    ) -> impl Future<Output = Result<Vec<String>, CatError>> {
        async move {
            let rev_deps: Vec<String> = sqlx::query_scalar(sql::SELECT_REVERSE_DEP)
                .bind(name)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(rev_deps)
        }
    }

    fn is_path_exist<P>(&mut self, path: P) -> impl Future<Output = Result<bool, CatError>>
    where
        P: AsRef<Path>,
    {
        async move {
            let (exists,): (i64,) = sqlx::query_as(sql::SELECT_EXIST_FILE)
                .bind(path.as_ref().to_string_lossy())
                .fetch_one(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(exists == 1)
        }
    }

    fn get_orphan_pacs(
        &mut self,
    ) -> impl Future<Output = Result<Vec<(i64, String, PacState)>, CatError>> {
        async {
            let rows = sqlx::query_as::<_, (i64, String, PacState)>(sql::SELECT_ORPHAN_PAC)
                .fetch_all(self.to_conn_type().await?.as_connection())
                .await?;
            Ok(rows)
        }
    }
}
