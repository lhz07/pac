use std::{
    collections::{BTreeSet, HashMap},
    fs,
    io::{BufReader, Read, Write},
    iter::zip,
    path::{Path, PathBuf},
    rc::Rc,
    sync::LazyLock,
};

use flate2::read::GzDecoder;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::header::CONTENT_LENGTH;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::{
    API_MIRROR, BOTTLES_MIRROR, CACHE_DIR, CLIENT_WITH_RETRY, PAC_PATH,
    database::local::{PacState, SqlTransaction},
    errors::{CatError, CloudError, RequestError},
    macos::{
        file::{CmpPath, remove_dir_force, remove_dir_recursively_force, remove_file_force},
        version::ARCH_OS,
    },
    package::{
        find_depend::{detect_conflicts, resolve_depend},
        install::install,
        preprocess::before_install,
    },
    scopeguard::DropGuard,
};

static PROGRESS_STYLE: LazyLock<ProgressStyle> = LazyLock::new(|| {
    ProgressStyle::default_bar()
        .template(
            "{prefix} {wide_bar} {bytes} / {total_bytes} ({binary_bytes_per_sec}  eta: {eta})",
        )
        .expect("progress template should be valid!")
});

#[derive(Debug, Deserialize, Clone)]
pub struct PacInfo {
    pub name: String,
    pub full_name: String,
    pub versions: Version,
    pub tap: String,
    pub bottle: Option<Bottle>,
    pub dependencies: Vec<String>,
    pub conflicts_with: Vec<String>,
    /// we don't support install multi versions for now
    pub versioned_formulae: Vec<String>,
    pub revision: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Version {
    pub stable: Option<String>,
    pub bottle: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Bottle {
    pub stable: Option<BottleInfo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BottleInfo {
    pub rebuild: u32,
    pub files: HashMap<String, File>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct File {
    pub cellar: String,
    pub url: String,
    pub sha256: String,
}

pub async fn get_json_api(name: &str) -> Result<PacInfo, CloudError> {
    let root = API_MIRROR
        .as_deref()
        .unwrap_or("https://formulae.brew.sh/api");
    let url = format!("{}/formula/{}.json", root, name);
    let response = CLIENT_WITH_RETRY.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        if status == 404 {
            return Err(CloudError::api("Can not find the formula"));
        } else {
            return Err(RequestError::Status(format!("brew api error: {}", status)))?;
        }
    }
    let response_text = response.text().await?;
    let pac_info: PacInfo = serde_json::from_str(&response_text)?;
    Ok(pac_info)
}

pub async fn get_json_api_multi<S>(names: &[S]) -> Result<Vec<PacInfo>, CloudError>
where
    S: AsRef<str>,
{
    let futs = names
        .iter()
        .map(|s| get_json_api(s.as_ref()))
        .collect::<Vec<_>>();
    let res = futures::future::join_all(futs)
        .await
        .into_iter()
        .collect::<Result<Vec<PacInfo>, CloudError>>()?;
    Ok(res)
}

pub async fn get_all_json_api() -> Result<Vec<PacInfo>, CloudError> {
    let url = "https://formulae.brew.sh/api/formula.json";
    let mut response = CLIENT_WITH_RETRY.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(RequestError::Status(format!("brew api error: {}", status)))?;
    }
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|l| l.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(CloudError::api("Can not get all json"))?;
    let progress = ProgressBar::new(content_length);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{wide_bar} {bytes} / {total_bytes} ({binary_bytes_per_sec}  eta: {eta})")
            .expect("progress template should be valid!"),
    );
    let mut json = Vec::new();
    while let Some(bytes) = response.chunk().await? {
        std::io::Write::write_all(&mut json, &bytes)?;
        progress.inc(bytes.len() as u64);
    }
    let pac_info: Vec<PacInfo> = serde_json::from_slice(&json)?;
    Ok(pac_info)
}

#[derive(Debug, Deserialize)]
struct Token {
    token: String,
}

async fn get_token(repo: &str, name: &str) -> Result<String, CloudError> {
    let url = format!("https://ghcr.io/token?service=ghcr.io&scope=repository:{repo}/{name}:pull");
    let res = CLIENT_WITH_RETRY.get(url).send().await?.text().await?;
    let json: Token = serde_json::from_str(&res)?;
    Ok(json.token)
}

async fn download(repo: &str, url: &str, name: &str, sha256: &str) -> Result<PathBuf, CatError> {
    let token = get_token(repo, name).await?;
    let mut response = CLIENT_WITH_RETRY.get(url).bearer_auth(token).send().await?;
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|l| l.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(CloudError::api("Can not download"))?;
    let mut path = CACHE_DIR.clone();
    path.push(format!("{name}.tar.gz"));
    let mut file = fs::File::create(&path)?;
    let progress = ProgressBar::new(content_length);
    progress.set_style(PROGRESS_STYLE.clone());
    while let Some(bytes) = response.chunk().await? {
        file.write_all(&bytes)?;
        progress.inc(bytes.len() as u64);
    }
    if verify_hash(&path, sha256)? {
        Ok(path)
    } else {
        Err(CatError::Hash(format!(
            "Hash mismatch for downloaded file: {:?}",
            path
        )))
    }
}

async fn download_with_bar(
    repo: &str,
    url: &str,
    name: &str,
    sha256: &str,
    pac: &PacInfo,
    progress: ProgressBar,
) -> Result<PathBuf, CatError> {
    let download_file_name = format!("{name}-{}.tar.gz", sha256);
    let mut path = CACHE_DIR.clone();
    path.push(download_file_name);
    if let Ok(true) = verify_hash(&path, sha256) {
        println!("{} is already downloaded", name);
        return Ok(path);
    }
    let version = if pac.revision > 0 {
        format!("{}_{}", pac.versions.stable.as_ref().unwrap(), pac.revision)
    } else {
        pac.versions.stable.as_ref().unwrap().to_string()
    };
    let build = if pac
        .bottle
        .as_ref()
        .unwrap()
        .stable
        .as_ref()
        .unwrap()
        .rebuild
        > 0
    {
        format!(
            "bottle.{}",
            &pac.bottle
                .as_ref()
                .unwrap()
                .stable
                .as_ref()
                .unwrap()
                .rebuild
        )
    } else {
        "bottle".to_string()
    };
    let mut response = match &*BOTTLES_MIRROR {
        Some(url) => {
            let url = format!("{url}/{name}-{version}.{}.{build}.tar.gz", ARCH_OS.as_str());
            CLIENT_WITH_RETRY.get(url).send().await?
        }
        None => {
            let token = get_token(repo, name).await?;
            CLIENT_WITH_RETRY.get(url).bearer_auth(token).send().await?
        }
    };
    if !response.status().is_success() {
        return Err(CatError::Cloud(CloudError::Request(RequestError::Status(
            format!("code {}", response.status()),
        ))));
    }
    let content_length = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|l| l.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or(CloudError::api("Can not download"))?;

    let mut file = tokio::fs::File::create(&path).await?;
    progress.set_length(content_length);
    progress.set_prefix(name.to_string());
    let progress = progress.with_finish(indicatif::ProgressFinish::Abandon);
    while let Some(bytes) = response.chunk().await? {
        file.write_all(&bytes).await?;
        progress.inc(bytes.len() as u64);
    }
    if verify_hash(&path, sha256)? {
        Ok(path)
    } else {
        Err(CatError::Hash(format!(
            "Hash mismatch for downloaded file: {:?}",
            path
        )))
    }
}

pub async fn download_multi(pacs: &Vec<Rc<PacInfo>>) -> Result<Vec<PathBuf>, CatError> {
    let multi_bar = MultiProgress::new();
    let mut futs = Vec::new();
    for pac in pacs.iter() {
        if let Some(bottle) = &pac.bottle
            && let Some(bottle) = &bottle.stable
            && let Some(file) = bottle.files.get(ARCH_OS.as_str())
        {
            let bar = ProgressBar::hidden();
            bar.set_style(PROGRESS_STYLE.clone());
            let bar = multi_bar.add(bar);
            let fut = download_with_bar(&pac.tap, &file.url, &pac.name, &file.sha256, &pac, bar);
            futs.push(fut);
        } else {
            return Err(CatError::Hash(format!(
                "Package {} has no stable bottle for {}",
                pac.full_name,
                ARCH_OS.as_str()
            )));
        }
    }
    let res = futures::future::join_all(futs)
        .await
        .into_iter()
        .collect::<Result<Vec<PathBuf>, CatError>>()?;
    Ok(res)
}

fn verify_hash(path: &PathBuf, expected_hash: &str) -> Result<bool, CatError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut hasher = Sha256::new();

    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let result = hasher.finalize();
    let result_array: &[u8] = &result;
    let expected_hash = hex::decode(expected_hash).map_err(|e| CatError::Hash(e.to_string()))?;
    Ok(result_array == expected_hash)
}

pub async fn install_pac(req_name: &str) -> Result<(), CatError> {
    let mut tx = SqlTransaction::new().await?;
    if let Some((_, state)) = tx.is_installed(req_name).await? {
        match state {
            PacState::Installed => {
                println!("Package {} is already installed", req_name);
                return Ok(());
            }
            PacState::Broken => {
                return Err(CatError::Pac(format!(
                    "package {} is broken, please uninstall it first",
                    req_name
                )));
            }
        }
    }
    let pac = get_json_api(req_name).await?;
    println!("resolving dependents...");
    let deps = resolve_depend(pac).await?;
    let mut to_install = Vec::new();
    for dep in deps {
        match tx.is_installed(&dep.name).await? {
            Some((_, state)) => {
                if let PacState::Broken = state {
                    return Err(CatError::Pac(format!(
                        "package {} is broken (required by {})\n\
                        Please uninstall it first",
                        dep.name, req_name
                    )));
                }
            }
            None => to_install.push(dep),
        }
    }
    println!("detecting conflicts...");
    detect_conflicts(&to_install, &mut tx).await?;
    println!("downloading pacs...");
    let paths = download_multi(&to_install).await?;
    let mut temp_paths = DropGuard::new(Vec::<PathBuf>::new(), |temp_paths| {
        // clean temp dir
        println!("cleaning temp dirs...");
        for p in temp_paths {
            let _ = remove_dir_recursively_force(&p).inspect_err(|e| {
                eprintln!(
                    "Warning: Can not clean temp path: {}, error: {e}",
                    p.display()
                )
            });
        }
        println!("temp dirs are removed!");
    });
    let mut restore_guard = DropGuard::new(Vec::<Vec<PathBuf>>::new(), |installed_files| {
        eprintln!("encounter an error, restoring install dir");
        // also remove dirs
        let mut dirs = BTreeSet::new();
        let pac_path = Path::new(PAC_PATH);
        for paths in installed_files.iter() {
            for p in paths.iter() {
                let mut ancestors = p.ancestors();
                // skip itself
                ancestors.next();
                while let Some(parent) = ancestors.next()
                    && !dirs.contains(&CmpPath(parent))
                    && parent != pac_path
                {
                    dirs.insert(CmpPath(parent));
                }
                if let Err(e) = remove_file_force(&p) {
                    eprintln!(
                        "Warning: Can not remove installed file: {}, error: {e}",
                        p.display()
                    )
                }
            }
        }
        for dir in dirs {
            if let Err(e) = remove_dir_force(&*dir) {
                eprintln!(
                    "Warning: Can not remove installed dir: {}, error: {e}",
                    dir.0.display()
                )
            }
        }
        println!("recovery finished!");
    });
    // install pacs
    for (pac, mut path) in zip(to_install, paths) {
        println!("installing {}", pac.full_name);
        println!("loading downloaded files");
        let downloaded_file = fs::File::open(&path)?;
        let gz = GzDecoder::new(BufReader::new(downloaded_file));
        let mut archive = tar::Archive::new(gz);
        path.set_extension("");
        path.set_extension("");
        let mut temp_dir = std::env::temp_dir().join(path.file_name().unwrap());
        let _ = remove_dir_recursively_force(&temp_dir);
        temp_paths.push(temp_dir.clone());
        println!("extracting...");
        archive.unpack(&temp_dir)?;
        let name_version = if pac.revision > 0 {
            format!(
                "{}/{}_{}",
                pac.name,
                pac.versions.stable.as_ref().unwrap(),
                pac.revision
            )
        } else {
            format!("{}/{}", pac.name, pac.versions.stable.as_ref().unwrap())
        };
        temp_dir.push(&name_version);
        println!("preprocessing...");
        before_install(&temp_dir, &name_version)?;
        println!("preprocess done, installing...");
        let installed_files = Vec::new();
        restore_guard.push(installed_files);
        let installed_files = restore_guard.last_mut().unwrap();
        // we should ensure the path is not conflicted before calling install.
        // implmentation is in the function below
        install(&temp_dir, installed_files, &mut tx).await?;
        tx.install_a_pac(
            &pac,
            pac.versions.stable.as_ref().unwrap(),
            &pac.bottle.as_ref().unwrap().stable.as_ref().unwrap(),
            &pac.bottle
                .as_ref()
                .unwrap()
                .stable
                .as_ref()
                .unwrap()
                .files
                .get(ARCH_OS.as_str())
                .unwrap()
                .sha256,
            pac.name == req_name,
            &installed_files,
        )
        .await?;
        println!("Package {} is installed now", pac.full_name);
    }
    tx.commit().await?;
    // IMPORTANT: cancel the drop guard
    restore_guard.into_inner();
    Ok(())
}

// pub async fn install_a_pac(name: &str) -> Result<(), CatError> {
//     let pac = get_json_api(name).await?;
//     if let Some(bottle) = pac.bottle
//         && let Some(bottle) = bottle.stable
//         && let Some(file) = bottle.files.get(ARCH_OS.as_str())
//     {
//         println!("Downloading {}", pac.full_name);
//         let path = download(&pac.tap, &file.url, name, &file.sha256).await?;
//         println!("Downloaded and verified: {:?}", path);
//         println!("extracting...");
//         let downloaded_file = fs::File::open(&path)?;
//         let gz = GzDecoder::new(BufReader::new(downloaded_file));
//         let mut archive = tar::Archive::new(gz);
//         let mut temp_dir = std::env::temp_dir().join(format!("{name}--{}", file.sha256));
//         // std::fs::remove_dir_all(&temp_dir)?;
//         archive.unpack(&temp_dir)?;
//         let name_version = format!("{}/{}", pac.name, pac.versions.stable.unwrap());
//         temp_dir.push(&name_version);
//         before_install(&temp_dir, &name_version)?;
//         println!("preprocess done, installing...");
//         install(&temp_dir)?;
//         println!("Package {} is installed now", pac.full_name);
//     }

//     Ok(())
// }

#[tokio::test]
async fn test_get_json_api() {
    let res = get_json_api("wgett").await;
    assert!(matches!(res, Err(CloudError::Api(_))));
    let res = get_json_api("wget").await;
    assert!(res.is_ok());
}

// #[tokio::test]
// async fn test_download_a_pac() {
//     let res = install_a_pac("wget").await;
//     println!("{:?}", res);
//     assert!(res.is_ok());
// }

#[tokio::test]
async fn test_get_all_json() {
    let list = get_all_json_api().await.unwrap();
    println!("len: {}", list.len());
}

#[tokio::test]
async fn test_download_multi() {
    let pac1 = get_json_api("fish").await.unwrap();
    let pac2 = get_json_api("xmake").await.unwrap();
    download_multi(&vec![Rc::new(pac1), Rc::new(pac2)])
        .await
        .unwrap();
}
